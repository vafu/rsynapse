use super::*;
use proj_model::GoalInfo;

fn task(instance: &str, key: &str, parent: &str, status: &str) -> GoalIdentityInfo {
    GoalIdentityInfo {
        instance_id: instance.into(),
        key: key.into(),
        goal: GoalInfo {
            id: key.into(),
            parent_id: parent.into(),
            status: status.into(),
            project_id: "project".into(),
            ..Default::default()
        },
        project_name: "Project".into(),
        created_at_ms: 1,
        observed_at_ms: 1,
        deleted_at_ms: 0,
        ancestor_ids: Vec::new(),
    }
}

#[test]
fn task_membership_uses_immutable_instances_and_can_be_many_to_many() {
    let note = Note {
        tags: vec![task_tag("old-instance"), task_tag("another-task")],
        ..Default::default()
    };
    assert!(matches(&note, Some("old-instance"), ""));
    assert!(matches(&note, Some("another-task"), ""));
    assert!(!matches(&note, Some("new-instance-with-same-key"), ""));
    assert!(matches(&note, None, ""));
}

#[test]
fn hierarchy_retains_open_children_of_completed_parents_and_terminates_cycles() {
    let tasks = vec![
        task("p", "parent", "", "completed"),
        task("c", "child", "parent", "planned"),
        task("a", "cycle-a", "cycle-b", "planned"),
        task("b", "cycle-b", "cycle-a", "planned"),
    ];
    let rows = task_rows(&tasks, "project", false);
    assert_eq!(rows.len(), 4);
    assert_eq!((rows[0].0.instance_id.as_str(), rows[0].1), ("p", 0));
    assert_eq!((rows[1].0.instance_id.as_str(), rows[1].1), ("c", 1));
}

#[test]
fn tag_parser_normalizes_capture_and_edit_input() {
    assert_eq!(
        parse_tags(" #topic/rust, task/123,topic/rust, ,"),
        vec!["task/123", "topic/rust"]
    );
}

#[test]
fn cancelled_tasks_are_finished_but_their_unfinished_children_remain_reachable() {
    let tasks = vec![
        task("p", "parent", "", "cancelled"),
        task("c", "child", "parent", "planned"),
        task("closed", "closed", "", "cancelled"),
    ];
    let visible: Vec<_> = task_rows(&tasks, "project", false)
        .into_iter()
        .map(|(task, depth)| (task.instance_id.as_str(), depth))
        .collect();
    assert_eq!(visible, vec![("p", 0), ("c", 1)]);
    assert_eq!(task_rows(&tasks, "project", true).len(), 3);
}
