use std::{process::Command, sync::Arc, time::Duration};

use tokio::sync::RwLock;
use workspace_dbus_api::{BUS_NAME, ROOT_PATH, window_path};
use zbus::{Connection, connection::Builder, interface, zvariant::OwnedObjectPath};

#[derive(Clone)]
struct Manager(Arc<RwLock<Vec<OwnedObjectPath>>>);

#[interface(name = "org.rsynapse.Workspace.Manager1")]
impl Manager {
    #[zbus(property)]
    fn connected(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn generation(&self) -> u64 {
        1
    }

    #[zbus(property)]
    async fn windows(&self) -> Vec<OwnedObjectPath> {
        self.0.read().await.clone()
    }
}

struct Window {
    id: String,
    title: Arc<RwLock<String>>,
    app: String,
}

#[interface(name = "org.rsynapse.Workspace.Window1")]
impl Window {
    #[zbus(property)]
    fn id(&self) -> &str {
        &self.id
    }

    #[zbus(property)]
    async fn title(&self) -> Vec<String> {
        vec![self.title.read().await.clone()]
    }

    #[zbus(property)]
    fn app_id(&self) -> Vec<String> {
        vec![self.app.clone()]
    }

    // Both windows deliberately share a PID; the first is focused, the second
    // owns the terminal challenge. Neither property may decide ownership.
    #[zbus(property)]
    fn pid(&self) -> Vec<i32> {
        vec![1234]
    }

    #[zbus(property)]
    fn focused(&self) -> bool {
        self.id == "other"
    }
}

#[test]
fn terminal_discovery() {
    if std::env::var_os("TERMINAL_ID_TEST_WORKER").is_none() {
        assert!(
            Command::new("dbus-run-session")
                .arg("--")
                .arg(std::env::current_exe().unwrap())
                .args(["--exact", "terminal_discovery", "--nocapture"])
                .env("TERMINAL_ID_TEST_WORKER", "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(10), verify())
                .await
                .expect("discovery made no progress")
                .unwrap();
        });
}

async fn verify() -> anyhow::Result<()> {
    let id = "opaque/α:007";
    let paths = Arc::new(RwLock::new(vec![window_path("other"), window_path(id)]));
    let title = Arc::new(RwLock::new("original".to_string()));
    let service = Builder::session()?
        .name(BUS_NAME)?
        .serve_at(ROOT_PATH, Manager(paths.clone()))?
        .serve_at(
            window_path("other"),
            Window {
                id: "other".into(),
                title: Arc::new(RwLock::new("unrelated".into())),
                app: "com.mitchellh.ghostty".into(),
            },
        )?
        .serve_at(
            window_path(id),
            Window {
                id: id.into(),
                title: title.clone(),
                app: "com.mitchellh.ghostty".into(),
            },
        )?
        .build()
        .await?;
    let client = Connection::session().await?;
    let window = service
        .object_server()
        .interface::<_, Window>(window_path(id))
        .await?;

    let discovered = workspace_terminal_id::resolve(&client, "fresh-token", || async {
        *title.write().await = "fresh-token".into();
        window
            .get()
            .await
            .title_changed(window.signal_context())
            .await?;
        Ok(())
    })
    .await?;
    assert_eq!(discovered, id);

    // The inventory can start empty: a new terminal may be projected only after
    // the OSC has been published. Wait for inventory signals, not polling.
    paths.write().await.clear();
    *title.write().await = "original".into();
    let manager = service
        .object_server()
        .interface::<_, Manager>(ROOT_PATH)
        .await?;
    let discovered = workspace_terminal_id::resolve(&client, "new-window-token", || async {
        *title.write().await = "new-window-token".into();
        paths.write().await.push(window_path(id));
        manager
            .get()
            .await
            .windows_changed(manager.signal_context())
            .await?;
        Ok(())
    })
    .await?;
    assert_eq!(discovered, id);

    // A matching browser title is not a terminal, and a different Ghostty title
    // must not be selected as fallback. Cancellation bounds this negative test.
    let nonterminal = "browser";
    service
        .object_server()
        .at(
            window_path(nonterminal),
            Window {
                id: nonterminal.into(),
                title: Arc::new(RwLock::new("browser-token".into())),
                app: "browser".into(),
            },
        )
        .await?;
    paths.write().await.push(window_path(nonterminal));
    assert!(
        tokio::time::timeout(
            Duration::from_millis(100),
            workspace_terminal_id::resolve(&client, "browser-token", || async { Ok(()) })
        )
        .await
        .is_err()
    );
    Ok(())
}
