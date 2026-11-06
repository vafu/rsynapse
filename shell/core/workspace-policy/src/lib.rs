//! UI-free workspace name/project/MRU/visibility policy.
//!
//! Shared by shell-source observables and the launcher workspaces plugin so the
//! two surfaces cannot disagree about names, visibility, ordering, or markers.
use std::collections::HashMap;

use locus::{RelationEndpoint, RelationRecord, keys};
use proj_model::ProjectInfo;

pub const PROJECT_RELATION: &str = "org.rsynapse.workspace.project";
pub const NAME_RELATION: &str = "org.rsynapse.workspace.name";
pub const NAME_KIND: &str = "org.rsynapse.workspace.name";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceItem {
    pub id: String,
    pub name: String,
    pub index: u32,
    pub focused: bool,
    pub has_windows: bool,
    pub output: String,
}

/// Preferred manual name from a Locus name record.
pub fn name_of(record: &RelationRecord) -> Option<String> {
    if record.metadata.get("source").map(String::as_str) != Some("manual") {
        return None;
    }
    let RelationEndpoint::StableKey { kind, id: _ } = &record.subject else {
        return None;
    };
    if kind != keys::WORKSPACE_ID {
        return None;
    }
    let RelationEndpoint::StableKey {
        kind: target_kind,
        id: name,
    } = &record.target
    else {
        return None;
    };
    if target_kind != NAME_KIND {
        return None;
    }
    let name = name.trim().to_owned();
    (!name.is_empty()).then_some(name)
}

/// Project id from a Locus workspace->project record.
pub fn project_id_of(record: &RelationRecord) -> Option<String> {
    let RelationEndpoint::StableKey { kind, .. } = &record.subject else {
        return None;
    };
    if kind != keys::WORKSPACE_ID {
        return None;
    }
    record
        .metadata
        .get("project-id")
        .cloned()
        .filter(|id| !id.is_empty())
}

pub fn workspace_names(records: &[RelationRecord]) -> HashMap<String, String> {
    records
        .iter()
        .filter_map(|record| {
            let id = workspace_id(&record.subject)?;
            Some((id, name_of(record)?))
        })
        .collect()
}

pub fn workspace_projects(records: &[RelationRecord]) -> HashMap<String, String> {
    records
        .iter()
        .filter_map(|record| {
            let id = workspace_id(&record.subject)?;
            Some((id, project_id_of(record)?))
        })
        .collect()
}

fn workspace_id(endpoint: &RelationEndpoint) -> Option<String> {
    match endpoint {
        RelationEndpoint::StableKey { kind, id } if kind == keys::WORKSPACE_ID => Some(id.clone()),
        _ => None,
    }
}

/// Anonymous empty non-focused workspaces are hidden; named, occupied, and
/// focused workspaces stay available.
pub fn visible(item: &WorkspaceItem) -> bool {
    item.has_windows || !item.name.is_empty() || item.focused
}

pub fn matches(item: &WorkspaceItem, words: &[&str], current_output: Option<&str>) -> bool {
    if current_output.is_some_and(|current| current != item.output) {
        return false;
    }
    let text = format!("{} {} {}", item.name, item.index, item.output).to_lowercase();
    words.iter().all(|word| text.contains(word))
}

/// Focused first, then MRU rank, then the rest.
pub fn sort_key(item: &WorkspaceItem, mru: &[String], focused_path: Option<&str>) -> usize {
    let path = workspace_dbus_api::workspace_path(&item.id);
    if focused_path == Some(path.as_str()) {
        return 0;
    }
    mru.iter()
        .position(|ranked| ranked.as_str() == path.as_str())
        .map(|rank| rank + 1)
        .unwrap_or(usize::MAX)
}

pub fn is_current(item: &WorkspaceItem, focused_path: Option<&str>) -> bool {
    let path = workspace_dbus_api::workspace_path(&item.id);
    focused_path == Some(path.as_str())
}

/// Title and icon for a workspace row, preferring an assigned project.
pub fn title_and_icon(item: &WorkspaceItem, project: Option<&ProjectInfo>) -> (String, String) {
    match project {
        Some(project) => (
            project.name.clone(),
            if project.icon.is_empty() {
                "folder-symbolic".to_owned()
            } else {
                project.icon.clone()
            },
        ),
        None => (
            if item.name.is_empty() {
                format!("Workspace {}", item.index)
            } else {
                item.name.clone()
            },
            "focus-windows-symbolic".to_owned(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(subject: RelationEndpoint, target: RelationEndpoint) -> RelationRecord {
        RelationRecord {
            subject,
            relation: NAME_RELATION.to_owned(),
            target,
            metadata: HashMap::from([("source".to_owned(), "manual".to_owned())]),
            created_at_unix_ms: 0,
            updated_at_unix_ms: 0,
        }
    }

    fn item(id: &str, name: &str, focused: bool, windows: bool, output: &str) -> WorkspaceItem {
        WorkspaceItem {
            id: id.to_owned(),
            name: name.to_owned(),
            index: 1,
            focused,
            has_windows: windows,
            output: output.to_owned(),
        }
    }

    #[test]
    fn only_manual_workspace_names_are_accepted() {
        let subject = RelationEndpoint::stable_key(keys::WORKSPACE_ID, "7");
        let target = RelationEndpoint::stable_key(NAME_KIND, "noble-owl");
        let mut entry = record(subject.clone(), target.clone());
        assert_eq!(name_of(&entry).as_deref(), Some("noble-owl"));
        entry
            .metadata
            .insert("source".to_owned(), "project".to_owned());
        assert_eq!(name_of(&entry), None);
        let other_target = RelationEndpoint::stable_key("other.kind", "x");
        assert_eq!(name_of(&record(subject, other_target)), None);
    }

    #[test]
    fn project_ids_come_from_workspace_metadata() {
        let mut entry = record(
            RelationEndpoint::stable_key(keys::WORKSPACE_ID, "7"),
            RelationEndpoint::stable_key(keys::PROJECT_PATH, "/p"),
        );
        entry.relation = PROJECT_RELATION.to_owned();
        entry
            .metadata
            .insert("project-id".to_owned(), "p1".to_owned());
        assert_eq!(project_id_of(&entry).as_deref(), Some("p1"));
        assert_eq!(
            workspace_projects(&[entry]).get("7").map(String::as_str),
            Some("p1")
        );
    }

    #[test]
    fn anonymous_empty_unfocused_workspaces_are_hidden() {
        assert!(!visible(&item("7", "", false, false, "DP-1")));
        assert!(visible(&item("7", "named", false, false, "DP-1")));
        assert!(visible(&item("7", "", true, false, "DP-1")));
        assert!(visible(&item("7", "", false, true, "DP-1")));
    }

    #[test]
    fn current_output_and_word_filters_match() {
        let entry = item("7", "noble-owl", false, true, "DP-1");
        assert!(matches(&entry, &["noble"], None));
        assert!(matches(&entry, &["dp-1"], None));
        assert!(!matches(&entry, &["other"], None));
        assert!(!matches(&entry, &[], Some("DP-2")));
        assert!(matches(&entry, &[], Some("DP-1")));
    }

    #[test]
    fn ordering_prefers_focus_then_mru() {
        let focused = item("7", "", true, false, "DP-1");
        let recent = item("8", "", false, true, "DP-1");
        let other = item("9", "", false, true, "DP-1");
        let focused_path = workspace_dbus_api::workspace_path("7").as_str().to_owned();
        let recent_path = workspace_dbus_api::workspace_path("8").as_str().to_owned();
        let mru = vec![recent_path];
        assert_eq!(sort_key(&focused, &mru, Some(&focused_path)), 0);
        assert_eq!(sort_key(&recent, &mru, Some(&focused_path)), 1);
        assert_eq!(sort_key(&other, &mru, Some(&focused_path)), usize::MAX);
        assert!(is_current(&focused, Some(&focused_path)));
        assert!(!is_current(&recent, Some(&focused_path)));
    }

    #[test]
    fn titles_prefer_projects_and_fall_back_to_names() {
        let entry = item("7", "", false, true, "DP-1");
        let (title, icon) = title_and_icon(&entry, None);
        assert_eq!(title, "Workspace 1");
        assert_eq!(icon, "focus-windows-symbolic");
        let named = item("7", "noble-owl", false, true, "DP-1");
        assert_eq!(title_and_icon(&named, None).0, "noble-owl");
        let project = ProjectInfo {
            id: "p".into(),
            name: "Project".into(),
            cwd: "/p".into(),
            checkout_id: String::new(),
            icon: String::new(),
            icon_origin: String::new(),
        };
        assert_eq!(
            title_and_icon(&entry, Some(&project)),
            ("Project".into(), "folder-symbolic".into())
        );
    }
}
