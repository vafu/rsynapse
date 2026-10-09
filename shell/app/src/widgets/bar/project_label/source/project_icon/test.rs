use super::*;
use futures_util::StreamExt;
use std::{
    os::unix::fs::PermissionsExt,
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
#[ignore = "requires PROJD_BIN and dbus-run-session; no GTK initialization"]
fn shell_autopicks_only_unset_project_icons() {
    if std::env::var_os("PROJECT_ICON_WIDGET_CHILD").is_none() {
        let temp = tempfile::tempdir().unwrap();
        let picker = temp.path().join("pick-icon");
        // A deterministic stand-in for the external fuzzy picker, with invocation evidence.
        std::fs::write(&picker, "#!/usr/bin/env python3\nimport os,pathlib\np=pathlib.Path(os.environ['ICON_PICK_LOG'])\np.write_text(p.read_text()+'picked\\n' if p.exists() else 'picked\\n')\nprint('[{\"glyph\":\"AUTO\",\"score\":1.0}]')\n").unwrap();
        std::fs::set_permissions(&picker, std::fs::Permissions::from_mode(0o755)).unwrap();
        let status = Command::new("dbus-run-session").arg("--")
            .arg(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "widgets::bar::project_label::source::project_icon::test::shell_autopicks_only_unset_project_icons", "--nocapture"])
            .env("PROJECT_ICON_WIDGET_CHILD", "1")
            .env("RSYNAPSE_PICK_ICON", &picker)
            .env("ICON_PICK_LOG", temp.path().join("calls"))
            .env("PROJD_STORE_PATH", temp.path().join("store.sqlite3"))
            .env("PROJECT_ICON_FIXTURE", temp.path())
            .status().unwrap();
        assert!(status.success());
        return;
    }
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
                .spawn()
                .unwrap(),
        );
        tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(event) = owners.next().await {
                if event.args().unwrap().name().as_str() == "org.rsynapse.Proj" {
                    break;
                }
            }
        })
        .await
        .unwrap();
        let manager = zbus::Proxy::new(
            &connection,
            "org.rsynapse.Proj",
            "/org/rsynapse/Proj",
            "org.rsynapse.Proj.Manager1",
        )
        .await
        .unwrap();
        let fixture = std::path::PathBuf::from(std::env::var_os("PROJECT_ICON_FIXTURE").unwrap());
        let dir = fixture.join("preset");
        std::fs::create_dir(&dir).unwrap();
        let preset: source::proj::ProjectInfo = manager
            .call("RegisterProject", &(dir.to_str().unwrap(),))
            .await
            .unwrap();
        let _: source::proj::ProjectInfo = manager
            .call("SetProjectIcon", &(&preset.id, "PRESET"))
            .await
            .unwrap();
        let project = ProjectDetails {
            has_project: true,
            project_id: Some(preset.id.clone()),
            name: Some("unique-preset-fixture".into()),
            ..ProjectDetails::default()
        };
        let mut stream = icon(project, preset.id).into_stream();
        let initial = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(initial.glyph, "PRESET");
        assert!(initial.overridden);
        let log = std::path::PathBuf::from(std::env::var_os("ICON_PICK_LOG").unwrap());
        assert!(
            !log.exists(),
            "loaded manual icon must not invoke automatic selection"
        );
        drop(stream);

        let dir = fixture.join("unset");
        std::fs::create_dir(&dir).unwrap();
        let unset: source::proj::ProjectInfo = manager
            .call("RegisterProject", &(dir.to_str().unwrap(),))
            .await
            .unwrap();
        let project = ProjectDetails {
            has_project: true,
            project_id: Some(unset.id.clone()),
            name: Some("unique-unset-fixture".into()),
            ..ProjectDetails::default()
        };
        let mut stream = icon(project.clone(), unset.id.clone()).into_stream();
        async fn expect(
            stream: &mut (impl futures_util::Stream<Item = Result<WorkspaceIcon, String>> + Unpin),
            glyph: &str,
            overridden: bool,
        ) -> WorkspaceIcon {
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let value = stream.next().await.unwrap().unwrap();
                    if value.glyph == glyph && value.overridden == overridden {
                        return value;
                    }
                }
            })
            .await
            .unwrap()
        }
        expect(&mut stream, "AUTO", false).await;
        let projects: Vec<source::proj::ProjectInfo> =
            manager.call("ListProjects", &()).await.unwrap();
        let stored = projects.iter().find(|p| p.id == unset.id).unwrap();
        assert_eq!(
            (stored.icon.as_str(), stored.icon_origin.as_str()),
            ("AUTO", "automatic")
        );
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "picked\n");
        // Resubscription reads persistence, rather than starting selection again.
        drop(stream);
        let mut stream = icon(project, unset.id.clone()).into_stream();
        expect(&mut stream, "AUTO", false).await;
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "picked\n");
        let _: source::proj::ProjectInfo = manager
            .call("SetProjectIcon", &(&unset.id, "MANUAL"))
            .await
            .unwrap();
        expect(&mut stream, "MANUAL", true).await;
        let _: source::proj::ProjectInfo = manager
            .call("ClearProjectIcon", &(&unset.id,))
            .await
            .unwrap();
        expect(&mut stream, "AUTO", false).await;
        // The heuristic result may be cached, but the cleared project gets it persisted again.
        let projects: Vec<source::proj::ProjectInfo> =
            manager.call("ListProjects", &()).await.unwrap();
        assert_eq!(
            projects.iter().find(|p| p.id == unset.id).unwrap().icon,
            "AUTO"
        );
    });
}
