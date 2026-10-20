use super::*;
use workspace_dbus_api::Layout;

fn workspace(id: &str, output: &str, index: u32) -> observe::Workspace {
    observe::Workspace {
        id: id.into(),
        output: output.into(),
        index,
        name: String::new(),
        layout: Layout::Scrolling,
    }
}
fn window(id: &str, home: &str) -> observe::Window {
    observe::Window {
        id: id.into(),
        app: "test".into(),
        pid: None,
        workspace: home.into(),
        floating: false,
        column: vec![0],
        row: vec![0],
    }
}
fn baseline() -> Baseline {
    let mut source = observe::Snapshot {
        owner: ":1.2".into(),
        generation: 3,
        outputs: BTreeSet::from(["removed".into(), "kept".into()]),
        workspaces: BTreeMap::new(),
        windows: BTreeMap::new(),
    };
    source
        .workspaces
        .insert("a".into(), workspace("a", "removed", 1));
    source
        .workspaces
        .insert("b".into(), workspace("b", "removed", 2));
    source
        .workspaces
        .insert("c".into(), workspace("c", "kept", 1));
    for (id, home) in [("one", "a"), ("two", "b"), ("three", "c")] {
        source.windows.insert(id.into(), window(id, home));
    }
    let workspaces = [("slot-a", "saved-one", 1), ("slot-b", "saved-two", 2)]
        .into_iter()
        .map(|(id, win, index)| model::Workspace {
            id: id.into(),
            binding: "same-project-checkpoint".into(),
            output: "removed".into(),
            index,
            name: String::new(),
            layout: Layout::Scrolling,
            windows: vec![model::Window {
                id: win.into(),
                app_id: "test".into(),
                floating: false,
                column: vec![0],
                row: vec![0],
                recipe: vec![model::Recipe {
                    provider: "configured".into(),
                    version: 1,
                    argv: vec!["never-launch".into()],
                    cwd: "/tmp".into(),
                    match_app_id: "test".into(),
                }],
            }],
        })
        .collect();
    Baseline {
        source,
        session: Session {
            version: 1,
            name: "current".into(),
            workspaces,
        },
        slots: BTreeMap::from([("slot-a".into(), "a".into()), ("slot-b".into(), "b".into())]),
        recovery_focus: None,
        windows: BTreeMap::from([
            ("saved-one".into(), "one".into()),
            ("saved-two".into(), "two".into()),
        ]),
    }
}
fn merged(base: &Baseline) -> observe::Snapshot {
    let mut current = base.source.clone();
    current.workspaces.retain(|_, w| w.output == "kept");
    current.outputs.remove("removed");
    for window in current.windows.values_mut() {
        window.workspace = "c".into();
    }
    current
}

#[test]
fn same_project_workspaces_remain_distinct_and_recovery_never_launches() {
    let base = baseline();
    let mut current = merged(&base);
    current.windows.get_mut("one").unwrap().floating = true;
    let recovery = plan(&base, &current, &[]).unwrap();
    assert_eq!(recovery.workspaces.len(), 2);
    assert_ne!(recovery.workspaces[0].id, recovery.workspaces[1].id);
    assert!(recovery.workspaces.iter().all(|w| w.output == "kept"));
    assert!(
        recovery
            .workspaces
            .iter()
            .flat_map(|w| &w.windows)
            .all(|w| w.recipe.is_empty())
    );
    assert!(recovery.workspaces[0].windows[0].floating);
    assert!(
        !base.session.workspaces[0].windows[0].recipe.is_empty(),
        "checkpoint retains launch recipe"
    );
}

#[test]
fn ordinary_window_moves_do_not_trigger_topology_repair() {
    let base = baseline();
    let mut current = base.source.clone();
    current.windows.get_mut("one").unwrap().workspace = "c".into();
    current.workspaces.remove("a"); // dynamic pruning after moving its last window
    assert!(lost_outputs(&base, &current).is_empty());
    assert!(plan(&base, &current, &[]).unwrap().workspaces.is_empty());
}

#[test]
fn native_preserved_groups_and_already_repaired_groups_are_not_moved_again() {
    let base = baseline();
    let mut current = merged(&base);
    current
        .workspaces
        .insert("new-a".into(), workspace("new-a", "kept", 2));
    current.windows.get_mut("one").unwrap().workspace = "new-a".into();
    let record = RelationRecord {
        subject: RelationEndpoint::stable_key(locus::keys::WORKSPACE_ID, "new-a"),
        relation: links::LIVE_SLOT.into(),
        target: RelationEndpoint::stable_key(links::SLOT, "slot-a"),
        metadata: links::metadata(Some(&current)),
        created_at_unix_ms: 0,
        updated_at_unix_ms: 0,
    };
    let recovery = plan(&base, &current, &[record]).unwrap();
    assert_eq!(recovery.workspaces.len(), 1);
    assert_eq!(recovery.workspaces[0].id, "slot-b");
    current
        .workspaces
        .insert("a".into(), workspace("a", "kept", 3));
    current.windows.get_mut("one").unwrap().workspace = "a".into();
    assert_eq!(plan(&base, &current, &[]).unwrap().workspaces.len(), 1);
}

#[test]
fn closed_windows_are_not_recreated_and_unresolved_membership_defers_repair() {
    let base = baseline();
    let mut current = merged(&base);
    current.windows.remove("one");
    assert_eq!(plan(&base, &current, &[]).unwrap().workspaces.len(), 1);
    current.windows.get_mut("two").unwrap().workspace.clear();
    assert!(plan(&base, &current, &[]).is_err());
}

#[test]
fn returning_output_wins_over_retained_disabled_fallback_objects() {
    let mut base = baseline();
    base.source = merged(&base);
    for (native, slot, window_id, index) in [
        ("fallback-a", "slot-a", "one", 2),
        ("fallback-b", "slot-b", "two", 3),
    ] {
        base.source
            .workspaces
            .insert(native.into(), workspace(native, "kept", index));
        base.source.windows.get_mut(window_id).unwrap().workspace = native.into();
        base.slots.insert(slot.into(), native.into());
    }
    let mut current = base.source.clone();
    current.outputs = BTreeSet::from(["removed".into()]);
    current
        .workspaces
        .insert("returned".into(), workspace("returned", "removed", 1));
    current.windows.get_mut("one").unwrap().workspace = "returned".into();
    current.windows.get_mut("two").unwrap().workspace = "returned".into();
    let records = ["slot-a", "slot-b"]
        .into_iter()
        .enumerate()
        .map(|(index, slot)| RelationRecord {
            subject: RelationEndpoint::stable_key(links::SLOT, slot),
            relation: links::OUTPUT_PREFERENCE.into(),
            target: RelationEndpoint::stable_key(locus::keys::OUTPUT_NAME, "removed"),
            metadata: std::collections::HashMap::from([
                ("index".into(), (index + 1).to_string()),
                ("fallback".into(), "true".into()),
            ]),
            created_at_unix_ms: 0,
            updated_at_unix_ms: 0,
        })
        .collect::<Vec<_>>();
    let recovery = plan(&base, &current, &records).unwrap();
    assert_eq!(recovery.workspaces.len(), 2);
    assert!(recovery.workspaces.iter().all(|w| w.output == "removed"));
    assert!(needs_reconciliation(&base, &current, &records));
}
