use super::*;

fn goal(id: &str, parent: &str, title: &str) -> GoalInfo {
    GoalInfo {
        id: id.into(),
        parent_id: parent.into(),
        title: title.into(),
        ..Default::default()
    }
}

#[test]
fn hierarchy_retains_parent_context_and_tracks_the_selected_subgoal() {
    let root = goal("root", "", "Z root");
    let child = goal("child", "root", "A child");
    let nested = goal("nested", "child", "Nested task");
    let active = child.object_path();
    let vm = view("p".into(), vec![nested, child, root], &active);
    assert_eq!(vm.title, "A child");
    assert!(vm.tooltip.starts_with("Z root › A child"));
    assert_eq!(vm.active, "child");
    assert_eq!(vm.context_keys, ["root", "child"]);
    assert_eq!(
        vm.rows
            .iter()
            .map(|r| (r.goal.id.as_str(), r.depth))
            .collect::<Vec<_>>(),
        vec![("root", 0), ("child", 1), ("nested", 2)]
    );
}

#[test]
fn nested_subgoals_show_their_ancestry_and_root_tasks_still_work() {
    let goals = vec![
        goal("root", "", "Objective"),
        goal("child", "root", "Milestone"),
        goal("nested", "child", "Current step"),
    ];
    let vm = view("p".into(), goals.clone(), &goals[2].object_path());
    assert_eq!(vm.title, "Current step");
    assert!(
        vm.tooltip
            .starts_with("Objective › Milestone › Current step")
    );
    assert_eq!(vm.active, "nested");
    assert_eq!(
        view("p".into(), goals.clone(), &goals[0].object_path()).title,
        "Objective"
    );
    assert_eq!(view("p".into(), goals, "/").title, "Choose goal");
}

#[test]
fn selected_parent_shows_its_unique_in_progress_subgoal_without_guessing_between_siblings() {
    let root = goal("root", "", "Objective");
    let mut first = goal("first", "root", "Current work");
    first.status = "in-progress".into();
    first.description = "Current step context".into();
    let mut second = goal("second", "root", "Other work");
    second.status = "planned".into();
    let vm = view(
        "p".into(),
        vec![root.clone(), first.clone(), second.clone()],
        &root.object_path(),
    );
    assert_eq!(vm.title, "Current work");
    assert!(vm.tooltip.starts_with("Objective › Current work"));
    assert_eq!(vm.description, "Current step context");
    assert_eq!(vm.active, "root");
    second.status = "in-progress".into();
    let goals = vec![root.clone(), first.clone(), second];
    assert_eq!(
        view("p".into(), goals.clone(), &root.object_path()).title,
        "Objective"
    );
    assert_eq!(
        view("p".into(), goals, &first.object_path()).title,
        "Current work"
    );
}

#[test]
#[ignore = "requires PROJD_BIN and dbus-run-session; no GTK initialization"]
fn ancestor_and_subgoal_titles_follow_live_properties() {
    use futures_util::StreamExt;
    use std::{
        collections::HashMap,
        process::{Child, Command},
        time::Duration,
    };
    if std::env::var_os("GOAL_CONTEXT_CHILD").is_none() {
        let status = Command::new("dbus-run-session").arg("--").arg(std::env::current_exe().unwrap())
            .args(["--ignored","--exact","widgets::bar::goals::source::test::ancestor_and_subgoal_titles_follow_live_properties","--nocapture"])
            .env("GOAL_CONTEXT_CHILD","1").status().unwrap();
        assert!(status.success());
        return;
    }
    struct Daemon(Child);
    impl Drop for Daemon {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let temp = tempfile::tempdir().unwrap();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
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
            let mut parent = goal("root", "", "Parent goal");
            parent.kind = "task".into();
            parent.priority = "medium".into();
            parent.status = "in-progress".into();
            let parent: GoalInfo = manager.call("CreateGoal", &(parent,)).await.unwrap();
            let mut child = goal("child", "root", "Current subgoal");
            child.kind = "task".into();
            child.priority = "medium".into();
            child.status = "in-progress".into();
            let child: GoalInfo = manager.call("CreateGoal", &(child,)).await.unwrap();
            let vm = view(
                "p".into(),
                vec![parent.clone(), child.clone()],
                &parent.object_path(),
            );
            let mut stream = tracked_context(vm).into_stream();
            async fn expect(
                stream: &mut (impl futures_util::Stream<Item = Result<GoalBarVm, String>> + Unpin),
                title: &str,
                context: &str,
            ) {
                let vm = tokio::time::timeout(Duration::from_secs(5), stream.next())
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();
                assert_eq!(vm.title, title);
                assert!(vm.tooltip.starts_with(context));
                assert_eq!(vm.active, "root");
            }
            expect(
                &mut stream,
                "Current subgoal",
                "Parent goal › Current subgoal",
            )
            .await;
            let _: GoalInfo = manager
                .call(
                    "EditGoal",
                    &(parent.key(), HashMap::from([("title", "Updated parent")])),
                )
                .await
                .unwrap();
            expect(
                &mut stream,
                "Current subgoal",
                "Updated parent › Current subgoal",
            )
            .await;
            let _: GoalInfo = manager
                .call(
                    "EditGoal",
                    &(child.key(), HashMap::from([("title", "Updated subgoal")])),
                )
                .await
                .unwrap();
            expect(
                &mut stream,
                "Updated subgoal",
                "Updated parent › Updated subgoal",
            )
            .await;
        });
}
