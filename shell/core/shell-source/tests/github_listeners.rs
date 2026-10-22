use std::{process::Command, sync::atomic::Ordering, time::Duration};

use futures_util::{Stream, StreamExt};
use shell_source::{github, rx::Observable as _};
use tokio::sync::mpsc;

#[path = "github_listeners/fake.rs"]
mod fake;
use fake::{Event, State};

// Each scenario is a subprocess on its own private bus. No environment changes
// race the shell-source process-global connection cache or other test threads.
fn private_bus(name: &str) -> bool {
    if std::env::var_os("SHELL_GITHUB_LISTENERS_CHILD").is_some() {
        return false;
    }
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("bus.conf");
    std::fs::write(&config, r#"<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*" eavesdrop="true"/><allow eavesdrop="true"/><allow own="*"/></policy></busconfig>"#).unwrap();
    let status = Command::new("dbus-run-session")
        .arg(format!("--config-file={}", config.display()))
        .arg("--")
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("SHELL_GITHUB_LISTENERS_CHILD", "1")
        .status()
        .unwrap();
    assert!(status.success(), "private-bus scenario {name} failed");
    true
}

async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("signal barrier timed out")
}

async fn next<T: std::fmt::Debug>(
    stream: &mut (impl Stream<Item = Result<T, String>> + Unpin),
) -> T {
    bounded(stream.next())
        .await
        .expect("stream ended")
        .expect("source failed")
}

async fn loaded(
    stream: &mut (impl Stream<Item = Result<Option<Vec<String>>, String>> + Unpin),
) -> Vec<String> {
    bounded(async {
        loop {
            if let Some(urls) = stream.next().await.unwrap().unwrap() {
                return urls;
            }
        }
    })
    .await
}

async fn created(events: &mut mpsc::UnboundedReceiver<Event>) -> (String, String, Vec<String>) {
    bounded(async {
        loop {
            if let Event::Created {
                path,
                creator,
                labels,
            } = events.recv().await.unwrap()
            {
                return (path, creator, labels);
            }
        }
    })
    .await
}

async fn closed(events: &mut mpsc::UnboundedReceiver<Event>, expected: &str) {
    bounded(async {
        loop {
            match events.recv().await.unwrap() {
                // No-reply Close and creator-disconnect can race at the server;
                // either cleanup path must remove the object. Ignore a second
                // cleanup event for a previous lease on this factory.
                Event::Closed(path) | Event::Reaped(path) if path == expected => return,
                _ => {}
            }
        }
    })
    .await
}

fn labels(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().unwrap()
}

#[test]
fn all_filter_sharing_updates_and_last_drop_close() {
    if private_bus("all_filter_sharing_updates_and_last_drop_close") {
        return;
    }
    runtime().block_on(async {
        let (state, mut events) = State::new(false, false);
        let service = fake::serve(state.clone(), true).await;
        let mut first = github::listener(labels(&["project", "ci", "project"])).into_stream();
        assert_eq!(next(&mut first).await, None);
        assert_eq!(loaded(&mut first).await, labels(&["https://test/pr/1"]));
        let (path, creator, filter) = created(&mut events).await;
        assert_eq!(filter, labels(&["ci", "project"]));
        assert_ne!(creator, service.unique_name().unwrap().as_str());
        let reads = state.reads.load(Ordering::SeqCst);

        let mut second = github::listener(labels(&["ci", "project"])).into_stream();
        // A late subscriber replays membership, without a new loading pulse.
        assert_eq!(
            next(&mut second).await,
            Some(labels(&["https://test/pr/1"]))
        );
        assert_eq!(state.creates.load(Ordering::SeqCst), 1);
        let urls = labels(&["https://test/pr/4", "https://test/pr/5"]);
        fake::update(&service, &path, urls.clone()).await;
        assert_eq!(next(&mut first).await, Some(urls.clone()));
        assert_eq!(next(&mut second).await, Some(urls));
        drop(first);
        fake::update(&service, &path, vec![]).await;
        assert_eq!(next(&mut second).await, Some(vec![]));
        assert_eq!(state.closes.load(Ordering::SeqCst), 0);
        assert_eq!(
            state.reads.load(Ordering::SeqCst),
            reads,
            "membership signals must carry values, not trigger reads"
        );
        assert_eq!(state.creates.load(Ordering::SeqCst), 1);
        drop(second);
        closed(&mut events, &path).await;
        assert!(state.active.lock().unwrap().is_empty());

        // No subscription remains: a later request must create a fresh lease.
        let mut fresh = github::listener(filter).into_stream();
        assert_eq!(next(&mut fresh).await, None);
        assert_eq!(loaded(&mut fresh).await, labels(&["https://test/pr/1"]));
        let (fresh_path, _, _) = created(&mut events).await;
        assert_ne!(fresh_path, path);
        drop(fresh);
        closed(&mut events, &fresh_path).await;
    });
}

#[test]
fn empty_filter_and_exact_labels_preserve_loaded_empty() {
    if private_bus("empty_filter_and_exact_labels_preserve_loaded_empty") {
        return;
    }
    runtime().block_on(async {
        let (state, mut events) = State::new(false, false);
        let _service = fake::serve(state.clone(), true).await;
        let mut all = github::listener(vec![]).into_stream();
        assert_eq!(next(&mut all).await, None);
        assert_eq!(
            loaded(&mut all).await,
            labels(&[
                "https://test/pr/1",
                "https://test/pr/2",
                "https://test/pr/3"
            ])
        );
        let (all_path, _, filter) = created(&mut events).await;
        assert!(filter.is_empty());
        let mut empty =
            github::listener(labels(&[" Project ", "project", "", " Project "])).into_stream();
        assert_eq!(next(&mut empty).await, None);
        assert_eq!(next(&mut empty).await, Some(vec![]));
        let (empty_path, _, filter) = created(&mut events).await;
        assert_eq!(filter, labels(&["", " Project ", "project"]));
        assert_eq!(state.creates.load(Ordering::SeqCst), 2);
        drop(all);
        closed(&mut events, &all_path).await;
        drop(empty);
        closed(&mut events, &empty_path).await;
    });
}

#[test]
fn bootstrap_membership_change_is_not_lost() {
    if private_bus("bootstrap_membership_change_is_not_lost") {
        return;
    }
    runtime().block_on(async {
        let (state, mut events) = State::new(false, true);
        let service = fake::serve(state.clone(), true).await;
        let mut stream = github::listener(labels(&["ci"])).into_stream();
        assert_eq!(next(&mut stream).await, None);
        let (path, _, _) = created(&mut events).await;
        bounded(state.read_started.acquire())
            .await
            .unwrap()
            .forget();
        let urls = labels(&["https://test/pr/new"]);
        // The fake getter has captured the old snapshot, but not replied.
        fake::update(&service, &path, urls.clone()).await;
        state.read_gate.as_ref().unwrap().add_permits(10);
        bounded(async {
            loop {
                if stream.next().await.unwrap().unwrap() == Some(urls.clone()) {
                    break;
                }
            }
        })
        .await;
        assert_eq!(state.creates.load(Ordering::SeqCst), 1);
        assert_eq!(state.reads.load(Ordering::SeqCst), 1);
        drop(stream);
        closed(&mut events, &path).await;
    });
}

#[test]
fn owner_loss_clears_and_recreates() {
    if private_bus("owner_loss_clears_and_recreates") {
        return;
    }
    runtime().block_on(async {
        let mut stream = github::listener(labels(&["project"])).into_stream();
        assert_eq!(next(&mut stream).await, None);
        let (state, mut events) = State::new(false, false);
        let first = fake::serve(state, true).await;
        assert_eq!(loaded(&mut stream).await.len(), 2);
        let (old_path, old_creator, _) = created(&mut events).await;
        first.release_name(github::BUS_NAME).await.unwrap();
        assert_eq!(next(&mut stream).await, None);
        closed(&mut events, &old_path).await;
        // The old unique owner remains connected, even reusing its object path.
        fake::emit(&first, &old_path, labels(&["stale"])).await;
        let (state, mut events) = State::new(false, false);
        let _second = fake::serve(state, true).await;
        assert_eq!(next(&mut stream).await.unwrap().len(), 2);
        let (path, creator, _) = created(&mut events).await;
        assert_ne!(creator, old_creator);
        drop(stream);
        closed(&mut events, &path).await;
    });
}

#[test]
fn direct_replacement_resets_listener_and_json_before_bootstrap() {
    if private_bus("direct_replacement_resets_listener_and_json_before_bootstrap") {
        return;
    }
    runtime().block_on(async {
        let (state, mut old_events) = State::new(false, false);
        let first = fake::serve(state, true).await;
        let mut stream = github::listener(labels(&["ci"])).into_stream();
        let mut overview = github::overview().into_stream();
        assert_eq!(loaded(&mut stream).await.len(), 1);
        bounded(async { while next(&mut overview).await.is_none() {} }).await;
        let (old_path, old_creator, _) = created(&mut old_events).await;
        let (state, mut events) = State::new(true, false);
        let second = fake::serve(state.clone(), false).await;
        second
            .request_name_with_flags(
                github::BUS_NAME,
                zbus::fdo::RequestNameFlags::ReplaceExisting
                    | zbus::fdo::RequestNameFlags::DoNotQueue,
            )
            .await
            .unwrap();
        assert_eq!(next(&mut stream).await, None);
        assert_eq!(next(&mut overview).await, None);
        closed(&mut old_events, &old_path).await;
        let (path, creator, _) = created(&mut events).await;
        assert_ne!(creator, old_creator);
        fake::emit(&first, &old_path, labels(&["stale"])).await;
        let mut late = github::listener(labels(&["ci"])).into_stream();
        assert_eq!(
            next(&mut late).await,
            None,
            "late subscribers must not replay the old owner's list"
        );
        state.create_gate.as_ref().unwrap().add_permits(1);
        assert_eq!(
            next(&mut stream).await,
            Some(labels(&["https://test/pr/1"]))
        );
        assert_eq!(next(&mut late).await, Some(labels(&["https://test/pr/1"])));
        assert_eq!(next(&mut overview).await.unwrap().open, 7);
        assert_eq!(state.creates.load(Ordering::SeqCst), 1);
        drop(stream);
        drop(late);
        closed(&mut events, &path).await;
    });
}

#[test]
fn cancelled_create_disconnects_creator_without_waiting_for_reply() {
    if private_bus("cancelled_create_disconnects_creator_without_waiting_for_reply") {
        return;
    }
    runtime().block_on(async {
        let (state, mut events) = State::new(true, false);
        let service = fake::serve(state.clone(), true).await;
        let mut stream = github::listener(labels(&["ci"])).into_stream();
        assert_eq!(next(&mut stream).await, None);
        let (path, creator, _) = created(&mut events).await;
        drop(stream);
        bounded(async {
            loop {
                if let Event::Reaped(reaped) = events.recv().await.unwrap() {
                    assert_eq!(reaped, path);
                    break;
                }
            }
        })
        .await;
        assert!(state.active.lock().unwrap().is_empty());
        assert_eq!(state.closes.load(Ordering::SeqCst), 0);
        let dbus = zbus::fdo::DBusProxy::new(&service).await.unwrap();
        assert!(
            !dbus
                .name_has_owner(creator.as_str().try_into().unwrap())
                .await
                .unwrap()
        );
        // Let the fake method finish only after cleanup has been proven.
        state.create_gate.as_ref().unwrap().add_permits(1);
    });
}
