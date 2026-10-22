use std::{process::Command, sync::Arc, time::Duration};

use futures_util::{Stream, StreamExt};
use shell_source::{StateSignal, proj, rx::Observable as _};
use tokio::sync::RwLock;

struct Project(Arc<RwLock<Vec<String>>>);

#[zbus::interface(name = "org.rsynapse.Proj.Project1")]
impl Project {
    #[zbus(property)]
    async fn labels(&self) -> Vec<String> {
        self.0.read().await.clone()
    }
}

async fn expect(
    stream: &mut (impl Stream<Item = Result<Option<Vec<String>>, String>> + Unpin),
    expected: Option<Vec<String>>,
) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if stream.next().await.unwrap().unwrap() == expected {
                break;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("project labels did not emit {expected:?}"));
}

#[test]
fn labels_seed_follow_scope_and_owner_changes() {
    if std::env::var_os("SHELL_PROJECT_LABEL_TEST_CHILD").is_none() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("bus.conf");
        std::fs::write(&config, r#"<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*" eavesdrop="true"/><allow eavesdrop="true"/><allow own="*"/></policy></busconfig>"#).unwrap();
        assert!(
            Command::new("dbus-run-session")
                .arg(format!("--config-file={}", config.display()))
                .arg("--")
                .arg(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "labels_seed_follow_scope_and_owner_changes",
                    "--nocapture"
                ])
                .env("SHELL_PROJECT_LABEL_TEST_CHILD", "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let a = Arc::new(RwLock::new(vec![]));
        let b = Arc::new(RwLock::new(vec!["beta".into()]));
        let path_a = proj_model::object_path("Projects", "a");
        let path_b = proj_model::object_path("Projects", "b");
        let serve = || async {
            zbus::connection::Builder::session()
                .unwrap()
                .serve_at(path_a.as_str(), Project(a.clone()))
                .unwrap()
                .serve_at(path_b.as_str(), Project(b.clone()))
                .unwrap()
                .name(proj_model::BUS_NAME)
                .unwrap()
                .build()
                .await
                .unwrap()
        };
        let selected = StateSignal::new("a".to_string());
        let mut stream =
            shell_source::switch_map(selected.observable(), proj::project_labels).into_stream();
        expect(&mut stream, None).await;
        let first = serve().await;
        expect(&mut stream, Some(vec![])).await;
        *a.write().await = vec!["alpha".into(), "team name".into()];
        let a_handle = first
            .object_server()
            .interface::<_, Project>(path_a.as_str())
            .await
            .unwrap();
        a_handle
            .get()
            .await
            .labels_changed(a_handle.signal_context())
            .await
            .unwrap();
        expect(&mut stream, Some(a.read().await.clone())).await;

        let mut late = proj::project_labels("a").into_stream();
        expect(&mut late, Some(a.read().await.clone())).await;
        selected.set("b".into());
        expect(&mut stream, None).await;
        expect(&mut stream, Some(vec!["beta".into()])).await;
        // A late old-project update must not appear in the selected scope.
        a.write().await.clear();
        a_handle
            .get()
            .await
            .labels_changed(a_handle.signal_context())
            .await
            .unwrap();
        expect(&mut late, Some(vec![])).await;
        *b.write().await = vec!["beta-next".into()];
        let b_handle = first
            .object_server()
            .interface::<_, Project>(path_b.as_str())
            .await
            .unwrap();
        b_handle
            .get()
            .await
            .labels_changed(b_handle.signal_context())
            .await
            .unwrap();
        expect(&mut stream, Some(vec!["beta-next".into()])).await;

        first.release_name(proj_model::BUS_NAME).await.unwrap();
        expect(&mut stream, None).await;
        expect(&mut late, None).await;
        *b.write().await = vec!["replacement".into()];
        let _replacement = serve().await;
        expect(&mut stream, Some(vec!["replacement".into()])).await;
        expect(&mut late, Some(vec![])).await;
    });
}
