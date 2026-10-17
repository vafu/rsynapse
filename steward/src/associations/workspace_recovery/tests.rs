use super::*;

fn source() -> workspace_cleanup::Snapshot {
    workspace_cleanup::Snapshot {
        owner: ":1.42".into(),
        generation: 1,
        connected: true,
        ids: HashSet::from(["new-workspace".into()]),
        windows: HashMap::from([("window-a".into(), Some("new-workspace".into()))]),
    }
}

fn record(kind: &str, id: &str, relation: &str, project: &str) -> RelationRecord {
    RelationRecord {
        subject: RelationEndpoint::stable_key(kind, id),
        relation: relation.into(),
        target: RelationEndpoint::stable_key(
            locus::keys::PROJECT_PATH,
            format!("/projects/{project}"),
        ),
        metadata: HashMap::from([
            ("managed-by".into(), "rsynapse-steward".into()),
            (OWNER.into(), ":1.42".into()),
            (GENERATION.into(), "1".into()),
            ("project-id".into(), project.into()),
        ]),
        created_at_unix_ms: 0,
        updated_at_unix_ms: 0,
    }
}

fn anchor(window: &str, project: &str) -> RelationRecord {
    record(
        locus::keys::WINDOW_ID,
        window,
        WINDOW_WORKSPACE_PROJECT,
        project,
    )
}

#[test]
fn surviving_window_recovers_a_recreated_workspace_without_recreating_project_metadata() {
    let mut snapshot = source();
    snapshot
        .windows
        .insert("window-b".into(), Some("new-workspace".into()));
    let changes = plan(
        &snapshot,
        &[anchor("window-a", "code"), anchor("window-b", "code")],
    );
    assert_eq!(
        changes,
        [Change::Restore {
            workspace: "new-workspace".into(),
            path: "/projects/code".into(),
            project: "code".into()
        }]
    );
}

#[test]
fn merged_projects_require_explicit_selection_instead_of_first_window_winning() {
    let mut snapshot = source();
    snapshot
        .windows
        .insert("window-b".into(), Some("new-workspace".into()));
    assert!(
        plan(
            &snapshot,
            &[anchor("window-a", "code"), anchor("window-b", "other")]
        )
        .is_empty()
    );
}

#[test]
fn an_explicit_workspace_binding_updates_anchors_and_wins_over_recovery() {
    let binding = record(
        locus::keys::WORKSPACE_ID,
        "new-workspace",
        WORKSPACE_PROJECT,
        "chosen",
    );
    assert_eq!(
        plan(&source(), &[anchor("window-a", "old"), binding.clone()]),
        [Change::Remember {
            window: "window-a".into(),
            target: binding.target,
            metadata: binding.metadata
        }]
    );
}

#[test]
fn new_windows_in_a_bound_workspace_get_anchors_even_without_agents() {
    let binding = record(
        locus::keys::WORKSPACE_ID,
        "new-workspace",
        WORKSPACE_PROJECT,
        "code",
    );
    assert!(
        matches!(&plan(&source(), &[binding])[..], [Change::Remember { window, .. }] if window == "window-a")
    );
}

#[test]
fn explicit_unassignment_clears_anchors_and_blocks_automatic_recovery() {
    let anchor = anchor("window-a", "code");
    let mut policy = record(
        locus::keys::WORKSPACE_ID,
        "new-workspace",
        WORKSPACE_PROJECT_POLICY,
        "none",
    );
    policy.metadata.insert("automatic".into(), "false".into());
    assert_eq!(
        plan(&source(), &[anchor.clone(), policy]),
        [Change::Forget(anchor.subject, anchor.target)]
    );
}

#[test]
fn source_epoch_change_or_window_close_cannot_transfer_stale_associations() {
    let anchor = anchor("window-a", "code");
    for snapshot in [
        workspace_cleanup::Snapshot {
            owner: ":1.99".into(),
            ..source()
        },
        workspace_cleanup::Snapshot {
            generation: 2,
            ..source()
        },
        workspace_cleanup::Snapshot {
            connected: false,
            ..source()
        },
        workspace_cleanup::Snapshot {
            windows: HashMap::new(),
            ..source()
        },
    ] {
        assert_eq!(
            plan(&snapshot, std::slice::from_ref(&anchor)),
            [Change::Forget(
                anchor.subject.clone(),
                anchor.target.clone()
            )]
        );
    }
}

#[test]
fn agent_cwd_project_links_are_not_mistaken_for_workspace_intent() {
    assert!(
        plan(
            &source(),
            &[record(
                locus::keys::WINDOW_ID,
                "window-a",
                super::super::WINDOW_PROJECT,
                "agent-cwd"
            )]
        )
        .is_empty()
    );
}
