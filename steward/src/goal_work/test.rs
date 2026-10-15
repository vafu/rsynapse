use super::*;
fn task(id: &str, parent: &str, selected: bool, status: &str) -> GoalTrackingInfo {
    GoalTrackingInfo {
        instance_id: id.into(),
        key: id.into(),
        title: id.into(),
        project_id: "p".into(),
        project_name: "P".into(),
        status: status.into(),
        parent_instance_id: parent.into(),
        selected,
        ancestor_ids: vec![],
    }
}
#[test]
fn focused_project_and_activity_control_leaf_attribution() {
    let mut state = FocusState {
        project_id: Some("p".into()),
        ..Default::default()
    };
    let tasks = vec![
        task("root", "", true, "in-progress"),
        task("child", "root", false, "in-progress"),
    ];
    assert_eq!(
        scope(&state, &tasks),
        Some(Scope {
            instance_id: "child".into(),
            project_id: "p".into(),
            kind: WorkKind::Active
        })
    );
    state.idle = true;
    assert_eq!(scope(&state, &tasks).unwrap().kind, WorkKind::Idle);
    state.locked = true;
    assert_eq!(scope(&state, &tasks), None);
    state.locked = false;
    state.project_id = Some("other".into());
    assert_eq!(scope(&state, &tasks), None);
}
#[test]
fn selection_resolves_ambiguity_without_counting_every_in_progress_task() {
    let state = FocusState {
        project_id: Some("p".into()),
        ..Default::default()
    };
    let mut tasks = vec![
        task("root", "", true, "in-progress"),
        task("a", "root", false, "in-progress"),
        task("b", "root", false, "in-progress"),
    ];
    assert_eq!(scope(&state, &tasks).unwrap().instance_id, "root");
    tasks[0].selected = false;
    tasks[2].selected = true;
    assert_eq!(scope(&state, &tasks).unwrap().instance_id, "b");
    tasks[2].status = "completed".into();
    assert_eq!(scope(&state, &tasks), None);
}

#[test]
#[ignore = "requires PROJD_BIN and dbus-run-session; private bus, no desktop actions"]
fn recorder_tracks_goal_idle_lock_and_service_restarts() {
    use futures_util::StreamExt;
    use goal_model::{GoalInfo, MANAGER_INTERFACE, ProjectInfo};
    use shell_source::rx::Observable as _;
    use std::{
        process::{Child, Command},
        time::Duration,
    };
    if std::env::var_os("GOAL_RECORDER_CHILD").is_none() {
        let result = Command::new("dbus-run-session")
            .arg("--")
            .arg(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "goal_work::test::recorder_tracks_goal_idle_lock_and_service_restarts",
                "--nocapture",
            ])
            .env("GOAL_RECORDER_CHILD", "1")
            .status()
            .unwrap();
        assert!(result.success());
        return;
    }
    struct Daemon(Child);
    impl Drop for Daemon {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let tmp = tempfile::tempdir().unwrap();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let conn = zbus::Connection::session().await.unwrap();
            let bus = zbus::fdo::DBusProxy::new(&conn).await.unwrap();
            let mut names = bus.receive_name_owner_changed().await.unwrap();
            let spawn = || {
                Daemon(
                    Command::new(std::env::var_os("PROJD_BIN").expect("PROJD_BIN"))
                        .env("PROJD_STORE_PATH", tmp.path().join("db.sqlite3"))
                        .spawn()
                        .unwrap(),
                )
            };
            let mut daemon = spawn();
            tokio::time::timeout(Duration::from_secs(10), async {
                while let Some(e) = names.next().await {
                    if e.args().unwrap().name().as_str() == BUS_NAME {
                        break;
                    }
                }
            })
            .await
            .unwrap();
            let manager = zbus::Proxy::new(&conn, BUS_NAME, ROOT_PATH, MANAGER_INTERFACE)
                .await
                .unwrap();
            let metrics = zbus::Proxy::new(&conn, BUS_NAME, ROOT_PATH, METRICS_INTERFACE)
                .await
                .unwrap();
            let project: ProjectInfo = manager
                .call("RegisterProject", &(tmp.path().to_str().unwrap(),))
                .await
                .unwrap();
            let root = GoalInfo {
                id: "root".into(),
                title: "Objective".into(),
                kind: "task".into(),
                status: "in-progress".into(),
                priority: "medium".into(),
                project: project.cwd.clone(),
                ..Default::default()
            };
            let _: GoalInfo = manager.call("CreateGoal", &(root,)).await.unwrap();
            let child = GoalInfo {
                id: "child".into(),
                title: "Current step".into(),
                parent_id: "root".into(),
                kind: "task".into(),
                status: "in-progress".into(),
                priority: "medium".into(),
                project: project.cwd,
                ..Default::default()
            };
            let _: GoalInfo = manager.call("CreateGoal", &(child,)).await.unwrap();
            let _: () = manager
                .call("SetActiveGoal", &(&project.id, "root"))
                .await
                .unwrap();
            let mut updates = shell_source::proj::goal_tracking().into_stream();
            let goals = tokio::time::timeout(Duration::from_secs(5), updates.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let mut recorder = Recorder::connect().await.unwrap();
            let mut state = FocusState {
                project_id: Some(project.id),
                ..Default::default()
            };
            recorder.observe(scope(&state, &goals)).await;
            async fn sessions(proxy: &zbus::Proxy<'_>) -> Vec<WorkSessionInfo> {
                proxy
                    .call(
                        "WorkTimeline",
                        &(String::new(), String::new(), 0u64, 0u64, false),
                    )
                    .await
                    .unwrap()
            }
            let rows = sessions(&metrics).await;
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].goal_key, "child");
            assert!(rows[0].open);
            state.idle = true;
            recorder.observe(scope(&state, &goals)).await;
            let rows = sessions(&metrics).await;
            assert_eq!(rows.len(), 2);
            assert!(!rows[0].open);
            assert_eq!(rows[1].kind, WorkKind::Idle);
            state.locked = true;
            recorder.observe(scope(&state, &goals)).await;
            assert!(sessions(&metrics).await.iter().all(|s| !s.open));
            state.locked = false;
            state.idle = false;
            recorder.observe(scope(&state, &goals)).await;
            let old = sessions(&metrics)
                .await
                .into_iter()
                .find(|s| s.open)
                .unwrap()
                .id;
            daemon.0.kill().unwrap();
            daemon.0.wait().unwrap();
            let empty = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let rows = updates.next().await.unwrap().unwrap();
                    if rows.is_empty() {
                        break rows;
                    }
                }
            })
            .await
            .unwrap();
            // Stop the local producer without activating the stopped service.
            recorder.current = None;
            assert!(scope(&state, &empty).is_none());
            daemon = spawn();
            let restored = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let rows = updates.next().await.unwrap().unwrap();
                    if !rows.is_empty() {
                        break rows;
                    }
                }
            })
            .await
            .unwrap();
            recorder.reconcile(scope(&state, &restored)).await;
            let rows = sessions(&metrics).await;
            assert!(
                rows.iter()
                    .any(|s| s.id == old && !s.open && s.end_reason.contains("unknown-duration"))
            );
            assert_eq!(rows.iter().filter(|s| s.open).count(), 1);
            recorder.finish().await;
            assert!(sessions(&metrics).await.iter().all(|s| !s.open));
            daemon.0.kill().unwrap();
            daemon.0.wait().unwrap();
        });
}
