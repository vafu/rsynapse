use super::*;
fn goal(id: &str, parent: &str, status: &str, priority: &str) -> GoalInfo {
    GoalInfo {
        id: id.into(),
        title: id.into(),
        parent_id: parent.into(),
        status: status.into(),
        priority: priority.into(),
        ..Default::default()
    }
}
fn info(goal: GoalInfo, at: u64) -> GoalIdentityInfo {
    GoalIdentityInfo {
        instance_id: goal.id.clone(),
        key: goal.key(),
        goal,
        created_at_ms: at,
        observed_at_ms: 100,
        deleted_at_ms: 0,
        project_name: String::new(),
        ancestor_ids: vec![],
    }
}

#[test]
fn siblings_sort_completion_then_priority_then_oldest_creation() {
    let goals = vec![
        goal("done", "", "completed", "high"),
        goal("low", "", "planned", "low"),
        goal("high-new", "", "planned", "high"),
        goal("high-old", "", "planned", "high"),
        goal("unknown", "", "planned", "high"),
    ];
    let ids = vec![
        info(goals[0].clone(), 1),
        info(goals[1].clone(), 2),
        info(goals[2].clone(), 30),
        info(goals[3].clone(), 10),
        info(goals[4].clone(), 0),
    ];
    assert_eq!(
        rows(&goals, &ids)
            .iter()
            .map(|r| r.goal.id.as_str())
            .collect::<Vec<_>>(),
        vec!["high-old", "high-new", "unknown", "low", "done"]
    );
}
#[test]
fn all_children_done_is_effective_completion_without_changing_status() {
    let goals = vec![
        goal("root", "", "in-progress", "high"),
        goal("branch", "root", "planned", "medium"),
        goal("leaf", "branch", "completed", "low"),
        goal("open", "", "planned", "low"),
    ];
    let r = rows(&goals, &[]);
    assert_eq!(
        r.iter().map(|r| r.goal.id.as_str()).collect::<Vec<_>>(),
        vec!["open", "root", "branch", "leaf"]
    );
    assert!(r[1].completed && r[2].completed);
    assert_eq!(r[1].goal.status, "in-progress");
    assert_eq!((r[1].completed_children, r[1].child_count), (1, 1));
}
#[test]
fn children_stay_under_their_parent_and_explicit_completion_wins() {
    let goals = vec![
        goal("parent", "", "completed", "low"),
        goal("open-child", "parent", "in-progress", "high"),
        goal("done-child", "parent", "completed", "high"),
        goal("open-root", "", "planned", "low"),
    ];
    let r = rows(&goals, &[]);
    assert_eq!(
        r.iter().map(|r| r.goal.id.as_str()).collect::<Vec<_>>(),
        vec!["open-root", "parent", "open-child", "done-child"]
    );
    assert!(r[1].completed);
    assert!(!r[2].completed);
}
