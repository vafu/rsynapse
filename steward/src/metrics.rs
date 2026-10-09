use std::{
    sync::mpsc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use futures_util::StreamExt;
use graphyne::{GraphiteClient, GraphiteMessage};
use locus::RelationRecord;
use shell_rx_macros::combine_latest;
use shell_source::{
    Observable,
    dbus::{self, Bus, ObjectDescriptor, PropertyDescriptor},
    rx::Observable as _,
};
use zbus::zvariant::OwnedObjectPath;

use crate::{
    focus::{FocusState, GraphiteSlots, Tracker},
    relations::{self, LocusClient},
};

const WORKSPACE_PATH_PREFIX: &str = "/org/rsynapse/Niri/Workspaces/workspace_";
const WINDOW_PATH_PREFIX: &str = "/org/rsynapse/Niri/Windows/window_";

mod agents;

fn prefix() -> String {
    std::env::var("METRIC_PREFIX").unwrap_or_else(|_| "rsynapse".to_owned())
}

pub struct Metrics {
    locus: LocusClient,
    carbon_host: String,
    carbon_port: u16,
}

impl Metrics {
    pub fn new(locus: LocusClient, carbon_host: String, carbon_port: u16) -> Self {
        Self {
            locus,
            carbon_host,
            carbon_port,
        }
    }

    pub async fn run(self, mut shutdown: tokio::sync::watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut workdays = crate::workdays::Workdays::load(&self.locus).await?;
        let mut workday_targets = crate::workdays::target_preferences(&self.locus).into_stream();
        let mut productivity = crate::productivity::Tracker::load(&self.locus).await?;
        let mut policies = crate::productivity::policies(&self.locus).into_stream();
        let (batch_sender, batch_receiver) = mpsc::channel::<Vec<(String, f64, u64)>>();
        let host = self.carbon_host.clone();
        let port = self.carbon_port;
        let carbon = std::thread::Builder::new()
            .name("carbon-push".to_owned())
            .spawn(move || run_carbon_push(batch_receiver, &host, port))
            .expect("spawn carbon push thread");

        let mut tracker = Tracker::new();
        let mut agents = crate::agent_metrics::AgentTracker::new();
        let mut usage_events = crate::agent_metrics::usage_events().into_stream();
        let mut agent_events = crate::agent_metrics::snapshots(&self.locus).into_stream();
        let mut states = activity_focus(
            effective_focus(focus_state(&self.locus), shell_source::session::locked()),
            shell_source::wayland::input_idle(input_idle_threshold()?),
        )
        .into_stream();
        // Events are the only reporting trigger. No interval, heartbeat,
        // debounce, or timed flush exists in this collection path.
        let result = loop {
            tokio::select! {
                biased;
                _ = shutdown.changed() => break Ok(()),
                item = states.next() => match item {
                    Some(Ok(state)) => {
                        productivity.focus(state.clone(), crate::productivity::tracker_now());
                        publish_pending(productivity.publish(&self.locus).await?, &batch_sender);
                        publish_pending(workdays.observe(&state, &self.locus).await?, &batch_sender);
                        agents.focus(state.window_id, !state.locked && !state.idle, Instant::now());
                        tracker.observe(state, Instant::now());
                        publish_events(&mut tracker, &batch_sender);
                        publish_pending(agents.drain(), &batch_sender);
                    }
                    Some(Err(error)) => break Err(anyhow::anyhow!("focus/activity source failed: {error}")),
                    None => break Err(anyhow::anyhow!("focus/activity source ended")),
                },
                item = usage_events.next() => match item {
                    Some(Ok(event)) => {
                        agents.usage(event);
                        publish_pending(agents.drain(), &batch_sender);
                    }
                    Some(Err(error)) => break Err(anyhow::anyhow!("agent usage source failed: {error}")),
                    None => break Err(anyhow::anyhow!("agent usage source ended")),
                },
                item = agent_events.next() => match item {
                    Some(Ok(snapshot)) => {
                        productivity.agents(snapshot.clone(), crate::productivity::tracker_now());
                        publish_pending(productivity.publish(&self.locus).await?, &batch_sender);
                        agents.update(snapshot, Instant::now());
                        publish_pending(agents.drain(), &batch_sender);
                    }
                    Some(Err(error)) => break Err(anyhow::anyhow!("agent source failed: {error}")),
                    None => break Err(anyhow::anyhow!("agent source ended")),
                },
                item = policies.next() => match item {
                    Some(Ok(policy)) => {
                        productivity.policy(policy, crate::productivity::tracker_now());
                        publish_pending(productivity.publish(&self.locus).await?, &batch_sender);
                    }
                    Some(Err(error)) => break Err(anyhow::anyhow!("productivity policy source failed: {error}")),
                    None => break Err(anyhow::anyhow!("productivity policy source ended")),
                },
                item = workday_targets.next() => match item {
                    Some(Ok(targets)) => publish_pending(workdays.set_targets(targets,&self.locus).await?,&batch_sender),
                    Some(Err(error)) => break Err(anyhow::anyhow!("workday target source failed: {error}")),
                    None => break Err(anyhow::anyhow!("workday target source ended")),
                }
            }
        };
        tracker.finish(Instant::now());
        agents.finish(Instant::now());
        productivity.finish(crate::productivity::tracker_now());
        publish_pending(productivity.publish(&self.locus).await?, &batch_sender);
        publish_pending(
            workdays
                .observe(
                    &FocusState {
                        locked: true,
                        ..FocusState::default()
                    },
                    &self.locus,
                )
                .await?,
            &batch_sender,
        );
        publish_events(&mut tracker, &batch_sender);
        publish_pending(agents.drain(), &batch_sender);
        drop(batch_sender);
        tokio::task::spawn_blocking(move || carbon.join())
            .await?
            .map_err(|_| anyhow::anyhow!("carbon push thread panicked"))?;
        result
    }
}

fn input_idle_threshold() -> anyhow::Result<Duration> {
    let seconds: u64 = std::env::var("INPUT_IDLE_SECS")
        .unwrap_or_else(|_| "30".to_owned())
        .parse()?;
    anyhow::ensure!(
        seconds > 0 && seconds <= u32::MAX as u64 / 1000,
        "INPUT_IDLE_SECS must be a positive Wayland timeout in seconds"
    );
    Ok(Duration::from_secs(seconds))
}

fn activity_focus(focus: Observable<FocusState>, idle: Observable<bool>) -> Observable<FocusState> {
    focus
        .combine_latest(idle, |state, idle| FocusState { idle, ..state })
        .distinct_until_changed()
        .box_it()
}

/// Lock changes independently terminate attribution, even without niri events.
/// Keep consuming niri while locked so unlocking resumes the latest focus.
fn effective_focus(
    focus: Observable<FocusState>,
    locked: Observable<bool>,
) -> Observable<FocusState> {
    focus
        .start_with(vec![FocusState::default()])
        .combine_latest(locked, |state, locked| {
            if locked {
                FocusState {
                    locked: true,
                    ..FocusState::default()
                }
            } else {
                state
            }
        })
        .distinct_until_changed()
        .box_it()
}

#[cfg(test)]
mod lock_tests {
    use super::*;
    use shell_source::rx::{ObservableFactory as _, Observer, Shared};

    #[tokio::test]
    async fn lock_without_niri_event_clears_focus_and_unlock_uses_latest() {
        let mut focus = Shared::subject::<FocusState, String>();
        let mut lock = Shared::subject::<bool, String>();
        let mut stream =
            effective_focus(focus.clone().box_it(), lock.clone().box_it()).into_stream();
        tokio::task::yield_now().await;
        lock.next(true);
        assert!(stream.next().await.unwrap().unwrap().locked);
        focus.next(FocusState {
            workspace_id: Some(10),
            ..FocusState::default()
        });
        lock.next(false);
        assert_eq!(stream.next().await.unwrap().unwrap().workspace_id, Some(10));
        lock.next(true);
        let cleared = stream.next().await.unwrap().unwrap();
        assert!(cleared.locked);
        assert!(cleared.workspace_id.is_none());
        focus.next(FocusState {
            workspace_id: Some(11),
            ..FocusState::default()
        });
        lock.next(false);
        let resumed = stream.next().await.unwrap().unwrap();
        assert!(!resumed.locked);
        assert_eq!(resumed.workspace_id, Some(11));
    }

    #[tokio::test]
    async fn idle_retains_focus_and_lock_still_clears_it() {
        let mut focus = Shared::subject::<FocusState, String>();
        let mut lock = Shared::subject::<bool, String>();
        let mut idle = Shared::subject::<bool, String>();
        let mut stream = activity_focus(
            effective_focus(focus.clone().box_it(), lock.clone().box_it()),
            idle.clone().box_it(),
        )
        .into_stream();
        tokio::task::yield_now().await;
        lock.next(false);
        idle.next(false);
        stream.next().await;
        focus.next(FocusState {
            workspace_id: Some(10),
            ..FocusState::default()
        });
        assert_eq!(stream.next().await.unwrap().unwrap().workspace_id, Some(10));
        idle.next(true);
        let paused = stream.next().await.unwrap().unwrap();
        assert!(paused.idle);
        assert_eq!(paused.workspace_id, Some(10));
        lock.next(true);
        assert!(stream.next().await.unwrap().unwrap().locked);
        lock.next(false);
        let still_idle = stream.next().await.unwrap().unwrap();
        assert!(still_idle.idle);
        assert!(!still_idle.locked);
        assert_eq!(still_idle.workspace_id, Some(10));
        focus.next(FocusState {
            workspace_id: Some(11),
            ..FocusState::default()
        });
        let moved_while_idle = stream.next().await.unwrap().unwrap();
        assert!(moved_while_idle.idle);
        assert_eq!(moved_while_idle.workspace_id, Some(11));
        idle.next(false);
        let resumed = stream.next().await.unwrap().unwrap();
        assert!(!resumed.idle);
        assert_eq!(resumed.workspace_id, Some(11));
    }
}

/// Full focus state: root focus paths combined, dynamic details resolved per
/// selection. Single process-wide subscriber; hubs stay connected while the
/// daemon runs, so workspace switches decode from signals with no re-reads.
fn focus_state(locus: &LocusClient) -> Observable<FocusState> {
    let selection = combine_latest!(
        focused_workspace(),
        focused_window(),
        focused_output()
        => |(workspace, window, output)| (workspace, window, output),
    )
    .distinct_until_changed()
    .box_it();
    let locus = locus.clone();
    shell_source::switch_map(selection, move |(workspace, window, output)| {
        let workspace = workspace.clone();
        let window = window.clone();
        // Rebuilt per switch on purpose: shared hubs dedupe by descriptor
        // key, so this reuses the live upstream instead of cloning streams.
        let projects = relations::records(locus.clone(), relations::WORKSPACE_PROJECT);
        let instances = relations::records(locus.clone(), relations::WINDOW_APP_INSTANCE);
        let agents = relations::records(locus.clone(), relations::WINDOW_AGENT_SESSION);
        let names = relations::records(locus.clone(), relations::WORKSPACE_NAME);
        combine_latest!(
            resolve_app(window.clone(), instances, agents),
            resolve_output_name(output.clone()),
            project_for(projects, workspace_id(&workspace)),
            explicit_name_for(names, workspace_id(&workspace))
            => move |(app, output_name, project, explicit_name)| FocusState {
                locked: false,
                idle: false,
                workspace_id: workspace_id(&workspace),
                // Explicit locus name wins; project display name covers
                // project-native workspaces; otherwise id-only.
                workspace_name: explicit_name.or(project.clone()),
                window_id: window_id(&window),
                app_id: app,
                project,
                output: output_name,
            },
        )
        .distinct_until_changed()
        .box_it()
    })
    .distinct_until_changed()
    .box_it()
}

fn focused_workspace() -> Observable<Option<OwnedObjectPath>> {
    dbus::optional_array_property::<OwnedObjectPath>(root_property("FocusedWorkspace"))
}

fn focused_window() -> Observable<Option<OwnedObjectPath>> {
    dbus::optional_array_property::<OwnedObjectPath>(root_property("FocusedWindow"))
}

fn focused_output() -> Observable<Option<OwnedObjectPath>> {
    dbus::optional_array_property::<OwnedObjectPath>(root_property("FocusedOutput"))
}

/// Canonical app identity: live agent identity first (opencode, codex, …),
/// then hook app-instance name (neovim, …), then canonicalized niri AppId.
fn resolve_app(
    window: Option<OwnedObjectPath>,
    instances: Observable<Vec<RelationRecord>>,
    agents: Observable<Vec<RelationRecord>>,
) -> Observable<Option<String>> {
    let Some(path) = window else {
        return shell_source::once(None);
    };
    let hook = hook_app_name(instances, window_id(&Some(path.clone())));
    let agent = agents::agent_app(agents, window_id(&Some(path.clone())));
    let app_id = dbus::optional_array_property::<String>(PropertyDescriptor::new(
        niri_object(path.as_str(), niri_dbus::WINDOW_INTERFACE),
        "AppId",
    ))
    .map(|app| app.map(|app| crate::focus::canonical_app_id(&app)));
    combine_latest!(
        agent,
        hook,
        app_id
        => move |(agent, hook, app_id)| agent.or(hook).or(app_id),
    )
    .distinct_until_changed()
    .box_it()
}

fn resolve_output_name(output: Option<OwnedObjectPath>) -> Observable<Option<String>> {
    let Some(path) = output else {
        return shell_source::once(None);
    };
    dbus::property_or(
        PropertyDescriptor::new(
            niri_object(path.as_str(), niri_dbus::OUTPUT_INTERFACE),
            "Name",
        ),
        String::new(),
    )
    .map(|name| (!name.is_empty()).then_some(name))
    .distinct_until_changed()
    .box_it()
}

fn project_for(
    records: Observable<Vec<RelationRecord>>,
    workspace_id: Option<u64>,
) -> Observable<Option<String>> {
    let Some(id) = workspace_id else {
        return shell_source::once(None);
    };
    let subject =
        locus::RelationEndpoint::stable_key(locus::keys::NIRI_WORKSPACE_ID, id.to_string());
    records
        .map(move |records| {
            records.into_iter().find_map(|record| {
                if record.subject != subject {
                    return None;
                }
                project_name(&record)
            })
        })
        .distinct_until_changed()
        .box_it()
}

fn hook_app_name(
    records: Observable<Vec<RelationRecord>>,
    window_id: Option<u64>,
) -> Observable<Option<String>> {
    let Some(id) = window_id else {
        return shell_source::once(None);
    };
    let subject = locus::RelationEndpoint::stable_key(locus::keys::NIRI_WINDOW_ID, id.to_string());
    records
        .map(move |records| {
            records.into_iter().find_map(|record| {
                if record.subject != subject {
                    return None;
                }
                record
                    .metadata
                    .get("app-name")
                    .cloned()
                    .filter(|name| !name.trim().is_empty())
            })
        })
        .distinct_until_changed()
        .box_it()
}

/// Explicit workspace name from the namer-owned relation, if any.
fn explicit_name_for(
    records: Observable<Vec<RelationRecord>>,
    workspace_id: Option<u64>,
) -> Observable<Option<String>> {
    let Some(id) = workspace_id else {
        return shell_source::once(None);
    };
    let subject =
        locus::RelationEndpoint::stable_key(locus::keys::NIRI_WORKSPACE_ID, id.to_string());
    records
        .map(move |records| {
            records.into_iter().find_map(|record| {
                if record.subject != subject {
                    return None;
                }
                let name = match &record.target {
                    locus::RelationEndpoint::StableKey { kind, id }
                        if kind == relations::WORKSPACE_NAME_KIND =>
                    {
                        id.clone()
                    }
                    _ => return None,
                };
                (!name.trim().is_empty()).then_some(name)
            })
        })
        .distinct_until_changed()
        .box_it()
}

fn project_name(record: &RelationRecord) -> Option<String> {
    record
        .metadata
        .get("name")
        .cloned()
        .filter(|name| !name.trim().is_empty())
        .or_else(|| {
            record.metadata.get("path").and_then(|path| {
                std::path::Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_owned)
            })
        })
}

fn root_property(name: &'static str) -> PropertyDescriptor {
    PropertyDescriptor::new(
        niri_object(niri_dbus::ROOT_PATH, niri_dbus::ROOT_INTERFACE),
        name,
    )
}

fn niri_object(path: &str, interface: &str) -> ObjectDescriptor {
    ObjectDescriptor::parse(Bus::Session, niri_dbus::BUS_NAME, path, interface)
        .expect("niri descriptor should be valid")
}

fn workspace_id(path: &Option<OwnedObjectPath>) -> Option<u64> {
    path.as_ref()?
        .as_str()
        .strip_prefix(WORKSPACE_PATH_PREFIX)?
        .parse()
        .ok()
}

fn window_id(path: &Option<OwnedObjectPath>) -> Option<u64> {
    path.as_ref()?
        .as_str()
        .strip_prefix(WINDOW_PATH_PREFIX)?
        .parse()
        .ok()
}

fn publish_events(tracker: &mut Tracker, batch_sender: &mpsc::Sender<Vec<(String, f64, u64)>>) {
    publish_pending(tracker.drain(), batch_sender);
}

fn publish_pending(
    pending: std::collections::HashMap<String, f64>,
    batch_sender: &mpsc::Sender<Vec<(String, f64, u64)>>,
) {
    if pending.is_empty() {
        return;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .unwrap_or(0);
    let batch = pending
        .into_iter()
        .map(|(metric, value)| (format!("{}.{metric}", prefix()), value, now))
        .collect::<Vec<_>>();
    if batch_sender.send(batch).is_err() {
        eprintln!("[rsynapse-steward/metrics] carbon push thread gone; dropping batch");
    }
}

fn run_carbon_push(receiver: mpsc::Receiver<Vec<(String, f64, u64)>>, host: &str, port: u16) {
    let mut client = None::<GraphiteClient>;
    let mut slots = GraphiteSlots::default();
    while let Ok(batch) = receiver.recv() {
        let batch = slots.event(batch);
        if client.is_none() {
            match GraphiteClient::builder().address(host).port(port).build() {
                Ok(built) => client = Some(built),
                Err(error) => {
                    eprintln!("[rsynapse-steward/metrics] carbon failed: {error}; dropping batch");
                    continue;
                }
            }
        }
        let Some(active) = client.as_mut() else {
            continue;
        };
        let messages = batch
            .iter()
            .map(|(path, value, timestamp)| {
                GraphiteMessage::new_with_ts(path, &value.to_string(), *timestamp)
            })
            .collect::<Vec<_>>();
        if let Err(error) = active.send_batch_message(&messages) {
            eprintln!("[rsynapse-steward/metrics] carbon failed: {error}; dropping batch");
            client = None;
        }
    }
}
