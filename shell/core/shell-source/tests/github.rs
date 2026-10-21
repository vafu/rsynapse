use std::{process::Command, sync::Arc, time::Duration};

use futures_util::StreamExt;
use shell_source::{github, rx::Observable as _};
use tokio::sync::RwLock;
use zbus::interface;

struct Service(
    Arc<RwLock<github::PrOverview>>,
    Arc<RwLock<Vec<github::PullRequest>>>,
    Arc<RwLock<Vec<github::PullRequestActivity>>>,
);

#[interface(name = "io.github.vfuchedzhy.GithubPr")]
impl Service {
    #[zbus(property)]
    async fn overview(&self) -> String {
        serde_json::to_string(&*self.0.read().await).unwrap()
    }

    #[zbus(property)]
    async fn pull_requests(&self) -> String {
        serde_json::to_string(&*self.1.read().await).unwrap()
    }

    #[zbus(property)]
    async fn pull_request_activity(&self) -> String {
        serde_json::to_string(&*self.2.read().await).unwrap()
    }
}

fn pr(number: u64, status: github::PrStatus) -> github::PullRequest {
    github::PullRequest {
        title: format!("PR {number}"),
        url: format!("https://example.test/repo/pull/{number}"),
        number,
        repo: "example/repo".into(),
        status,
        cool_status: None,
        merged_at: None,
        review_decision: None,
        mergeable: None,
        merge_state_status: None,
        checks: None,
        updated_at: String::new(),
        unresolved_threads: 0,
        latest_review: None,
    }
}

#[test]
fn overview_seeds_updates_and_survives_owner_restart() {
    if std::env::var_os("SHELL_GITHUB_TEST_CHILD").is_none() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("bus.conf");
        std::fs::write(&config, r#"<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*" eavesdrop="true"/><allow eavesdrop="true"/><allow own="*"/></policy></busconfig>"#).unwrap();
        let status = Command::new("dbus-run-session")
            .arg(format!("--config-file={}", config.display()))
            .arg("--")
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "overview_seeds_updates_and_survives_owner_restart",
                "--nocapture",
            ])
            .env("SHELL_GITHUB_TEST_CHILD", "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let current = Arc::new(RwLock::new(github::PrOverview {
            open: 5,
            merging: 2,
            ..Default::default()
        }));
        let prs = Arc::new(RwLock::new(vec![pr(1, github::PrStatus::Merging)]));
        let activity = Arc::new(RwLock::new(vec![github::PullRequestActivity {
            pr: pr(1, github::PrStatus::Merging),
            revision: "first".into(),
            unseen: false,
        }]));
        let serve = || async {
            zbus::connection::Builder::session()
                .unwrap()
                .serve_at(
                    github::PR_PATH,
                    Service(current.clone(), prs.clone(), activity.clone()),
                )
                .unwrap()
                .name(github::BUS_NAME)
                .unwrap()
                .build()
                .await
                .unwrap()
        };
        let mut stream = github::overview().into_stream();
        let mut list = github::pull_requests().into_stream();
        let mut activity_stream = github::activity().into_stream();
        async fn unavailable<T>(
            stream: &mut (impl futures_util::Stream<Item = Result<Option<T>, String>> + Unpin),
        ) {
            tokio::time::timeout(Duration::from_secs(5), async {
                while stream.next().await.unwrap().unwrap().is_some() {}
            })
            .await
            .expect("absent GitHub service retained stale counts");
        }
        unavailable(&mut stream).await;
        unavailable(&mut list).await;
        unavailable(&mut activity_stream).await;
        let first = serve().await;
        async fn expect<T: std::fmt::Debug + PartialEq>(
            stream: &mut (impl futures_util::Stream<Item = Result<Option<T>, String>> + Unpin),
            expected: &T,
        ) {
            let mut observed = Vec::new();
            let result = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let value = stream.next().await.unwrap().unwrap();
                    let matched = value.as_ref() == Some(expected);
                    observed.push(value);
                    if matched {
                        break;
                    }
                }
            })
            .await;
            assert!(
                result.is_ok(),
                "expected {expected:?}, observed {observed:?}"
            );
        }
        expect(&mut stream, &current.read().await.clone()).await;
        expect(&mut list, &prs.read().await.clone()).await;
        expect(&mut activity_stream, &activity.read().await.clone()).await;
        let next = github::PrOverview {
            open: 6,
            failed: 3,
            ci_failed: 2,
            merge_failed: 1,
            ..Default::default()
        };
        *current.write().await = next.clone();
        let next_prs = vec![
            pr(1, github::PrStatus::CiFailed),
            pr(2, github::PrStatus::Approved),
        ];
        *prs.write().await = next_prs.clone();
        let mut next_activity: Vec<_> = next_prs
            .iter()
            .map(|pr| github::PullRequestActivity {
                pr: pr.clone(),
                revision: format!("new-{}", pr.number),
                unseen: true,
            })
            .collect();
        *activity.write().await = next_activity.clone();
        let interface = first
            .object_server()
            .interface::<_, Service>(github::PR_PATH)
            .await
            .unwrap();
        interface
            .get()
            .await
            .overview_changed(interface.signal_context())
            .await
            .unwrap();
        interface
            .get()
            .await
            .pull_requests_changed(interface.signal_context())
            .await
            .unwrap();
        expect(&mut stream, &next).await;
        expect(&mut list, &next_prs).await;
        interface
            .get()
            .await
            .pull_request_activity_changed(interface.signal_context())
            .await
            .unwrap();
        expect(&mut activity_stream, &next_activity).await;
        // Acknowledgment changes activity without modifying the PR snapshot.
        next_activity[0].unseen = false;
        *activity.write().await = next_activity.clone();
        interface
            .get()
            .await
            .pull_request_activity_changed(interface.signal_context())
            .await
            .unwrap();
        expect(&mut activity_stream, &next_activity).await;
        // A second rail receives the existing current value immediately.
        let mut late = github::overview().into_stream();
        let mut late_list = github::pull_requests().into_stream();
        let mut late_activity = github::activity().into_stream();
        expect(&mut late, &next).await;
        expect(&mut late_list, &next_prs).await;
        expect(&mut late_activity, &next_activity).await;
        first.release_name(github::BUS_NAME).await.unwrap();
        unavailable(&mut stream).await;
        unavailable(&mut list).await;
        unavailable(&mut activity_stream).await;
        let after_restart = github::PrOverview {
            open: 7,
            merging: 1,
            approved: 4,
            ..Default::default()
        };
        *current.write().await = after_restart.clone();
        prs.write().await.clear();
        activity.write().await.clear();
        let _second = serve().await;
        expect(&mut stream, &after_restart).await;
        expect(&mut late, &after_restart).await;
        expect(&mut list, &Vec::new()).await;
        expect(&mut late_list, &Vec::new()).await;
        expect(&mut activity_stream, &Vec::new()).await;
        expect(&mut late_activity, &Vec::new()).await;
    });
}
