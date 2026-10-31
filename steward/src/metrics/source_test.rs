use super::*;
use std::{process::Command, sync::Arc};
use tokio::sync::{RwLock, Semaphore};

const A: &str = "/test/workspaces/a";
const B: &str = "/test/workspaces/b";
const WINDOW: &str = "/test/windows/b";

struct Selection {
    workspace: Vec<OwnedObjectPath>,
    window: Vec<OwnedObjectPath>,
}
struct Manager(Arc<RwLock<Selection>>);
#[zbus::interface(name = "org.rsynapse.Workspace.Manager1")]
impl Manager {
    #[zbus(property)]
    async fn focused_workspace(&self) -> Vec<OwnedObjectPath> {
        self.0.read().await.workspace.clone()
    }
    #[zbus(property)]
    async fn focused_window(&self) -> Vec<OwnedObjectPath> {
        self.0.read().await.window.clone()
    }
    #[zbus(property)]
    fn focused_output(&self) -> Vec<OwnedObjectPath> {
        vec![]
    }
}
struct Workspace(&'static str);
#[zbus::interface(name = "org.rsynapse.Workspace.Workspace1")]
impl Workspace {
    #[zbus(property(emits_changed_signal = "const"))]
    fn id(&self) -> &str {
        self.0
    }
}
struct Window {
    gate: Arc<Semaphore>,
    started: Arc<Semaphore>,
}
#[zbus::interface(name = "org.rsynapse.Workspace.Window1")]
impl Window {
    #[zbus(property(emits_changed_signal = "const"))]
    fn id(&self) -> &str {
        "opaque-window-b"
    }
    #[zbus(property)]
    async fn app_id(&self) -> Vec<String> {
        self.started.add_permits(1);
        self.gate.acquire().await.unwrap().forget();
        vec!["com.mitchellh.ghostty".into()]
    }
}
struct Relations;
#[zbus::interface(name = "org.rsynapse.Locus.Relations1")]
impl Relations {
    fn list(&self, relation: &str) -> Vec<RelationRecord> {
        if relation == relations::WORKSPACE_PROJECT {
            vec![
                context::test::binding("opaque-a", "p1"),
                context::test::binding("opaque-b", "p2"),
            ]
        } else {
            vec![]
        }
    }
}
struct Projects;
#[zbus::interface(name = "org.rsynapse.Proj.Manager1")]
impl Projects {
    fn list_projects(&self) -> Vec<shell_source::proj::ProjectInfo> {
        vec![
            context::test::project("p1", "rsynapse"),
            context::test::project("p2", "other"),
        ]
    }
}

async fn expect(
    stream: &mut (impl futures_util::Stream<Item = Result<FocusState, String>> + Unpin),
    workspace: &str,
    app: Option<&str>,
) -> FocusState {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let state = stream.next().await.unwrap().unwrap();
            if state.workspace_id.as_deref() == Some(workspace)
                && state.project.is_some()
                && (app.is_none() || state.app_id.as_deref() == app)
            {
                return state;
            }
        }
    })
    .await
    .expect("workspace/project focus stalled behind optional app metadata")
}

#[test]
fn workspace_time_and_project_names_do_not_wait_for_app_metadata() {
    if std::env::var_os("STEWARD_FOCUS_SOURCE_CHILD").is_none() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("bus.conf");
        std::fs::write(&config,r#"<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*" eavesdrop="true"/><allow eavesdrop="true"/><allow own="*"/></policy></busconfig>"#).unwrap();
        assert!(Command::new("dbus-run-session").arg(format!("--config-file={}",config.display()))
            .arg("--").arg(std::env::current_exe().unwrap())
            .args(["--exact","metrics::source_test::workspace_time_and_project_names_do_not_wait_for_app_metadata","--nocapture"])
            .env("STEWARD_FOCUS_SOURCE_CHILD","1").status().unwrap().success());
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let selection = Arc::new(RwLock::new(Selection {
            workspace: vec![A.try_into().unwrap()],
            window: vec![],
        }));
        let gate = Arc::new(Semaphore::new(0));
        let started = Arc::new(Semaphore::new(0));
        let server = zbus::connection::Builder::session()
            .unwrap()
            .serve_at(workspace_dbus_api::ROOT_PATH, Manager(selection.clone()))
            .unwrap()
            .serve_at(A, Workspace("opaque-a"))
            .unwrap()
            .serve_at(B, Workspace("opaque-b"))
            .unwrap()
            .serve_at(
                WINDOW,
                Window {
                    gate: gate.clone(),
                    started: started.clone(),
                },
            )
            .unwrap()
            .serve_at(locus::OBJECT_PATH, Relations)
            .unwrap()
            .serve_at(goal_model::ROOT_PATH, Projects)
            .unwrap()
            .name(workspace_dbus_api::BUS_NAME)
            .unwrap()
            .name(locus::BUS_NAME)
            .unwrap()
            .name(goal_model::BUS_NAME)
            .unwrap()
            .build()
            .await
            .unwrap();
        let locus = LocusClient::connect().await.unwrap();
        let mut states = focus_state(&locus).into_stream();
        let first = expect(&mut states, "opaque-a", None).await;
        assert_eq!(first.project.as_deref(), Some("rsynapse"));
        assert_eq!(first.workspace_name.as_deref(), Some("rsynapse"));
        let mut tracker = Tracker::new();
        let begin = Instant::now();
        tracker.observe(first, begin);
        tracker.drain();
        {
            let mut state = selection.write().await;
            state.workspace = vec![B.try_into().unwrap()];
            state.window = vec![WINDOW.try_into().unwrap()];
        }
        let manager = server
            .object_server()
            .interface::<_, Manager>(workspace_dbus_api::ROOT_PATH)
            .await
            .unwrap();
        manager
            .get()
            .await
            .focused_window_changed(manager.signal_context())
            .await
            .unwrap();
        // Deliver window focus first and prove its metadata read is blocked
        // before the independent workspace notification is delivered.
        tokio::time::timeout(Duration::from_secs(5), started.acquire())
            .await
            .unwrap()
            .unwrap()
            .forget();
        // A window update must not rebuild/temporarily erase the established
        // workspace/project context, even while its app name is pending.
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let state = states.next().await.unwrap().unwrap();
                assert_eq!(state.workspace_id.as_deref(), Some("opaque-a"));
                assert_eq!(state.project.as_deref(), Some("rsynapse"));
                assert_eq!(state.workspace_name.as_deref(), Some("rsynapse"));
                if state.window_id.as_deref() == Some("opaque-window-b") {
                    break;
                }
            }
        })
        .await
        .unwrap();
        manager
            .get()
            .await
            .focused_workspace_changed(manager.signal_context())
            .await
            .unwrap();
        let second = expect(&mut states, "opaque-b", None).await;
        assert_eq!(second.project.as_deref(), Some("other"));
        // Getter is still held pending, but workspace attribution has advanced.
        assert!(second.app_id.is_none());
        tracker.observe(second, begin + Duration::from_secs(300));
        let elapsed = tracker.drain();
        assert_eq!(
            elapsed.get("focus.project.rsynapse.idle.false.seconds"),
            Some(&300.0)
        );
        assert_eq!(
            elapsed.get("focus.workspace_name.rsynapse.idle.false.seconds"),
            Some(&300.0)
        );
        gate.add_permits(20);
        let enriched = expect(&mut states, "opaque-b", Some("ghostty")).await;
        tracker.observe(enriched, begin + Duration::from_secs(301));
        tracker.finish(begin + Duration::from_secs(600));
        let elapsed = tracker.drain();
        assert_eq!(
            elapsed.get("focus.project.other.idle.false.seconds"),
            Some(&300.0)
        );
        assert_eq!(
            elapsed.get("focus.workspace_name.other.idle.false.seconds"),
            Some(&300.0)
        );
    });
}
