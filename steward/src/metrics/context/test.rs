use super::*;
use crate::focus::{FocusState, Tracker};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

pub(in crate::metrics) fn project(id: &str, name: &str) -> ProjectInfo {
    ProjectInfo {
        id: id.into(),
        name: name.into(),
        cwd: format!("/projects/{id}"),
        checkout_id: String::new(),
        icon: String::new(),
        icon_origin: String::new(),
    }
}

pub(in crate::metrics) fn binding(workspace: &str, project: &str) -> RelationRecord {
    RelationRecord {
        subject: RelationEndpoint::stable_key(locus::keys::WORKSPACE_ID, workspace),
        relation: relations::WORKSPACE_PROJECT.into(),
        target: RelationEndpoint::stable_key(
            locus::keys::PROJECT_PATH,
            format!("/projects/{project}"),
        ),
        metadata: HashMap::from([("project-id".into(), project.into())]),
        created_at_unix_ms: 0,
        updated_at_unix_ms: 0,
    }
}

fn name(workspace: &str, value: &str, source: &str) -> RelationRecord {
    RelationRecord {
        subject: RelationEndpoint::stable_key(locus::keys::WORKSPACE_ID, workspace),
        relation: relations::WORKSPACE_NAME.into(),
        target: RelationEndpoint::stable_key(relations::WORKSPACE_NAME_KIND, value),
        metadata: HashMap::from([("source".into(), source.into())]),
        created_at_unix_ms: 0,
        updated_at_unix_ms: 0,
    }
}

#[test]
fn id_only_links_resolve_authoritative_names_and_renames_without_stale_mirrors() {
    let mut link = binding("opaque-A", "p1");
    link.metadata.insert("name".into(), "wrong-mirror".into());
    let names = vec![name("opaque-A", "stale-auto", "project")];
    let mut projects = vec![project("p1", "rsynapse")];
    let context = resolve("opaque-A", &[link.clone()], &names, &projects);
    assert_eq!(context.project_id.as_deref(), Some("p1"));
    assert_eq!(context.project.as_deref(), Some("rsynapse"));
    assert_eq!(context.workspace_name.as_deref(), Some("rsynapse"));
    projects[0].name = "new stored name".into();
    let renamed = resolve("opaque-A", &[link], &names, &projects);
    assert_eq!(renamed.project.as_deref(), Some("new stored name"));
    assert_eq!(renamed.workspace_name, renamed.project);
}

#[test]
fn manual_names_override_only_workspace_and_unassigned_spaces_are_empty() {
    let links = vec![binding("a", "p")];
    let projects = vec![project("p", "rsynapse")];
    let manual = resolve("a", &links, &[name("a", " office ", "manual")], &projects);
    assert_eq!(manual.workspace_name.as_deref(), Some("office"));
    assert_eq!(manual.project.as_deref(), Some("rsynapse"));
    assert_eq!(
        resolve("unassigned", &links, &[], &projects)
            .workspace_name
            .as_deref(),
        Some("empty")
    );
    let missing = resolve("a", &links, &[], &[]);
    assert_eq!(missing.project_id.as_deref(), Some("p"));
    assert!(missing.project.is_none());
    assert!(missing.workspace_name.is_none());
}

#[test]
fn long_focus_intervals_are_attributed_to_both_project_and_workspace() {
    let links = vec![binding("a", "p1"), binding("b", "p2")];
    let projects = vec![project("p1", "rsynapse"), project("p2", "other")];
    let focus = |id: &str| {
        let context = resolve(id, &links, &[], &projects);
        FocusState {
            workspace_id: Some(id.into()),
            project: context.project,
            project_id: context.project_id,
            workspace_name: context.workspace_name,
            ..Default::default()
        }
    };
    let start = Instant::now();
    let mut tracker = Tracker::new();
    tracker.observe(focus("a"), start);
    tracker.drain();
    tracker.observe(focus("b"), start + Duration::from_secs(300));
    let first = tracker.drain();
    assert_eq!(
        first.get("focus.project.rsynapse.idle.false.seconds"),
        Some(&300.0)
    );
    assert_eq!(
        first.get("focus.workspace_name.rsynapse.idle.false.seconds"),
        Some(&300.0)
    );
    let mut enriched = focus("b");
    enriched.app_id = Some("ghostty".into());
    tracker.observe(enriched, start + Duration::from_secs(301));
    tracker.finish(start + Duration::from_secs(600));
    let second = tracker.drain();
    assert_eq!(
        second.get("focus.project.other.idle.false.seconds"),
        Some(&300.0)
    );
    assert_eq!(
        second.get("focus.workspace_name.other.idle.false.seconds"),
        Some(&300.0)
    );
}
