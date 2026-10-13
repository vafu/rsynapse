use futures_util::StreamExt;
use proj_model::{BUS_NAME, GoalInfo, MANAGER_INTERFACE, ProjectInfo, ROOT_PATH};
use shell_source::{StateSignal, proj, rx::Observable as _, switch_map};
use std::{
    collections::HashMap,
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
fn active_goals_follow_properties_and_project_switches() {
    if std::env::var_os("GOALS_SOURCE_CHILD").is_none() {
        let status = Command::new("dbus-run-session")
            .arg("--")
            .arg(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "active_goals_follow_properties_and_project_switches",
                "--nocapture",
            ])
            .env("GOALS_SOURCE_CHILD", "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
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
            let mut projects = Vec::new();
            let mut goals = Vec::new();
            for (id, title) in [("first", "First task"), ("second", "Second task")] {
                let dir = temp.path().join(id);
                std::fs::create_dir(&dir).unwrap();
                let project: ProjectInfo = manager
                    .call("RegisterProject", &(dir.to_str().unwrap(),))
                    .await
                    .unwrap();
                let goal = GoalInfo {
                    id: id.into(),
                    title: title.into(),
                    kind: "task".into(),
                    status: "planned".into(),
                    priority: "medium".into(),
                    project: project.cwd.clone(),
                    ..Default::default()
                };
                let goal: GoalInfo = manager.call("CreateGoal", &(goal,)).await.unwrap();
                assert!(std::path::Path::new(&goal.note_path).is_file());
                let _: () = manager
                    .call("SetActiveGoal", &(&project.id, goal.key()))
                    .await
                    .unwrap();
                projects.push(project);
                goals.push(goal);
            }
            let selected = StateSignal::new(projects[0].id.clone());
            let active = switch_map(selected.observable(), |id| {
                proj::active_goal(id)
                    .combine_latest(proj::goals(), |path, goals| {
                        goals
                            .iter()
                            .find(|g| g.object_path() == path.as_str())
                            .map(|g| g.key())
                    })
                    .distinct_until_changed()
                    .box_it()
            });
            let mut titles = switch_map(active, |key| {
                key.map(proj::goal_title)
                    .unwrap_or_else(|| shell_source::once("No active goal".into()))
            })
            .into_stream();
            async fn expect(
                stream: &mut (impl futures_util::Stream<Item = Result<String, String>> + Unpin),
                value: &str,
            ) {
                let actual = tokio::time::timeout(Duration::from_secs(5), stream.next())
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();
                assert_eq!(actual, value);
            }
            expect(&mut titles, "First task").await;
            let _: GoalInfo = manager
                .call(
                    "EditGoal",
                    &(
                        goals[0].key(),
                        HashMap::from([("title", "Renamed task"), ("description", "More context")]),
                    ),
                )
                .await
                .unwrap();
            expect(&mut titles, "Renamed task").await;
            let mut description = proj::goal_description(goals[0].key()).into_stream();
            expect(&mut description, "More context").await;
            selected.set(projects[1].id.clone());
            expect(&mut titles, "Second task").await;
            let _: GoalInfo = manager
                .call(
                    "EditGoal",
                    &(goals[0].key(), HashMap::from([("title", "OLD PROJECT")])),
                )
                .await
                .unwrap();
            let _: GoalInfo = manager
                .call(
                    "EditGoal",
                    &(
                        goals[1].key(),
                        HashMap::from([("title", "Current project")]),
                    ),
                )
                .await
                .unwrap();
            expect(&mut titles, "Current project").await;
            let mut status = proj::goal_status(goals[1].key()).into_stream();
            expect(&mut status, "planned").await;
            let _: GoalInfo = manager
                .call("SetGoalStatus", &(goals[1].key(), "completed"))
                .await
                .unwrap();
            expect(&mut status, "completed").await;
            expect(&mut titles, "No active goal").await;
        });
}
