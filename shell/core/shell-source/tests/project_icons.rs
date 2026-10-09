//! Run with PROJD_BIN pointing to a freshly built daemon; uses a private bus.
use futures_util::StreamExt;
use proj_model::{BUS_NAME, MANAGER_INTERFACE, ProjectInfo, ROOT_PATH};
use shell_source::{StateSignal, proj, rx::Observable as _, switch_map};
use std::{
    process::{Child, Command},
    time::Duration,
};

struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[ignore = "requires PROJD_BIN and dbus-run-session"]
fn project_icons_seed_follow_changes_and_switch_projects() {
    if std::env::var_os("PROJD_ICON_TEST_CHILD").is_none() {
        let result = Command::new("dbus-run-session")
            .arg("--")
            .arg(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "project_icons_seed_follow_changes_and_switch_projects",
                "--nocapture",
            ])
            .env("PROJD_ICON_TEST_CHILD", "1")
            .status()
            .unwrap();
        assert!(result.success());
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let connection = zbus::Connection::session().await.unwrap();
        let bus = zbus::fdo::DBusProxy::new(&connection).await.unwrap();
        let mut owners = bus.receive_name_owner_changed().await.unwrap();
        let _daemon = Daemon(
            Command::new(std::env::var_os("PROJD_BIN").expect("set PROJD_BIN"))
                .env("PROJD_STORE_PATH", temp.path().join("store.sqlite3"))
                .spawn()
                .unwrap(),
        );
        tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(event) = owners.next().await {
                if event.args().unwrap().name().as_str() == BUS_NAME {
                    break;
                }
            }
        })
        .await
        .unwrap();
        let manager = zbus::Proxy::new(&connection, BUS_NAME, ROOT_PATH, MANAGER_INTERFACE)
            .await
            .unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        let a: ProjectInfo = manager
            .call("RegisterProject", &(first.to_str().unwrap(),))
            .await
            .unwrap();
        let b: ProjectInfo = manager
            .call("RegisterProject", &(second.to_str().unwrap(),))
            .await
            .unwrap();
        let _: ProjectInfo = manager.call("SetProjectIcon", &(&a.id, "A")).await.unwrap();
        let _: ProjectInfo = manager.call("SetProjectIcon", &(&b.id, "B")).await.unwrap();
        let associated = StateSignal::new(a.id.clone());
        let mut icons = switch_map(associated.observable(), proj::project_icon).into_stream();
        async fn expect(
            stream: &mut (impl futures_util::Stream<Item = Result<String, String>> + Unpin),
            expected: &str,
        ) {
            let value = tokio::time::timeout(Duration::from_secs(5), stream.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(value, expected);
        }
        expect(&mut icons, "A").await;
        let _: ProjectInfo = manager
            .call("SetProjectIcon", &(&a.id, "A2"))
            .await
            .unwrap();
        expect(&mut icons, "A2").await;
        // A late subscriber receives the cached current property immediately.
        let mut late = proj::project_icon(a.id.clone()).into_stream();
        expect(&mut late, "A2").await;
        associated.set(b.id.clone());
        expect(&mut icons, "B").await;
        let _: ProjectInfo = manager
            .call("SetProjectIcon", &(&a.id, "OLD"))
            .await
            .unwrap();
        let _: ProjectInfo = manager
            .call("SetProjectIcon", &(&b.id, "B2"))
            .await
            .unwrap();
        expect(&mut icons, "B2").await;
        expect(&mut late, "OLD").await;
        let result: ProjectInfo = manager
            .call("SetProjectIconIfUnset", &(&b.id, "AUTO"))
            .await
            .unwrap();
        assert_eq!(result.icon, "B2");
        let _: ProjectInfo = manager.call("ClearProjectIcon", &(&b.id,)).await.unwrap();
        expect(&mut icons, "").await;
        let _: ProjectInfo = manager
            .call("SetProjectIconIfUnset", &(&b.id, "AUTO"))
            .await
            .unwrap();
        expect(&mut icons, "AUTO").await;
        // Manual selection wins even if an automatic resolution finishes later.
        let _: ProjectInfo = manager
            .call("SetProjectIcon", &(&b.id, "MANUAL"))
            .await
            .unwrap();
        expect(&mut icons, "MANUAL").await;
        let result: ProjectInfo = manager
            .call("SetProjectIconIfUnset", &(&b.id, "STALE"))
            .await
            .unwrap();
        assert_eq!(
            (result.icon.as_str(), result.icon_origin.as_str()),
            ("MANUAL", "manual")
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(200), icons.next())
                .await
                .is_err()
        );
    });
}
