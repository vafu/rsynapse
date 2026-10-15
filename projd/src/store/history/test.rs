use super::*;

#[test]
fn renamed_deleted_and_recreated_aliases_retain_distinct_history() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("history.sqlite3");
    let store = Store::open(&path).unwrap();
    let mut goal = GoalInfo {
        id: "alias".into(),
        title: "Original".into(),
        status: "planned".into(),
        kind: "task".into(),
        ..Default::default()
    };
    assert!(store.save_goal(&goal, true, 100).unwrap());
    let old = store.identities().unwrap()[0].instance_id.clone();
    goal.title = "Renamed".into();
    goal.status = "in-progress".into();
    store.save_goal(&goal, false, 200).unwrap();
    store.select_goal("p", "alias", 250).unwrap();
    store.delete_goal_record(&goal, 300).unwrap();
    goal.title = "Different task".into();
    goal.status = "planned".into();
    assert!(store.save_goal(&goal, true, 400).unwrap());
    drop(store);
    let store = Store::open(&path).unwrap();
    let identities = store.identities().unwrap();
    assert_eq!(identities.len(), 2);
    assert_ne!(identities[0].instance_id, identities[1].instance_id);
    let events = store.history(&old, 0, 1000).unwrap();
    assert_eq!(
        events.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(),
        vec![
            "created",
            "status-changed",
            "selected",
            "unselected",
            "deleted"
        ]
    );
    assert_eq!(events[0].goal.title, "Original");
    assert_eq!(events.last().unwrap().goal.title, "Renamed");
}
#[test]
fn imported_parent_child_baselines_are_order_independent_and_partial() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open(&tmp.path().join("db")).unwrap();
    let root = GoalInfo {
        id: "z-parent".into(),
        status: "in-progress".into(),
        ..Default::default()
    };
    let child = GoalInfo {
        id: "a-child".into(),
        parent_id: root.key(),
        ..Default::default()
    };
    store.put("goal", &root.key(), &root).unwrap();
    store.put("goal", &child.key(), &child).unwrap();
    store.initialize_history(500).unwrap();
    store.initialize_history(600).unwrap();
    let identities = store.identities().unwrap();
    let parent = identities.iter().find(|g| g.key == root.key()).unwrap();
    let child = identities.iter().find(|g| g.key == child.key()).unwrap();
    assert_eq!(parent.created_at_ms, 0);
    assert_eq!(parent.observed_at_ms, 500);
    assert_eq!(child.ancestor_ids, vec![parent.instance_id.clone()]);
    assert_eq!(store.history("", 0, 1000).unwrap().len(), 2);
}
