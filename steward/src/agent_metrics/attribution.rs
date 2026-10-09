use super::Session;
use locus::{RelationEndpoint, RelationRecord};
use std::{collections::HashMap, path::Path};

pub(super) fn attribute(
    mut sessions: Vec<Session>,
    windows: &HashMap<u64, u64>,
    projects: &[RelationRecord],
    names: &[RelationRecord],
) -> Vec<Session> {
    for session in &mut sessions {
        let workspace = session.window.and_then(|id| windows.get(&id)).copied();
        session.workspace_id = workspace;
        let linked = workspace.and_then(|id| {
            projects
                .iter()
                .find(|r| workspace_id(&r.subject) == Some(id))
        });
        // Most specific known project containing the session cwd wins.
        let from_cwd = projects
            .iter()
            .filter_map(|r| {
                let root = project_path(r)?;
                (!session.cwd.is_empty() && Path::new(&session.cwd).starts_with(root))
                    .then_some((root.components().count(), r))
            })
            .max_by_key(|(length, _)| *length)
            .map(|(_, r)| r);
        session.project = from_cwd
            .or(linked)
            .and_then(project_name)
            .unwrap_or_else(|| "unassigned".to_owned());
        session.workspace = workspace
            .and_then(|id| names.iter().find(|r| workspace_id(&r.subject) == Some(id)))
            .and_then(|r| match &r.target {
                RelationEndpoint::StableKey { id, .. } => Some(id.clone()),
                _ => None,
            })
            .or_else(|| linked.and_then(project_name))
            .unwrap_or_else(|| session.project.clone());
    }
    // Children without a usable window/cwd inherit their parent's attribution.
    // Resolve a bounded number of rounds to support nested subagent trees.
    for _ in 0..sessions.len() {
        let parents: HashMap<_, _> = sessions
            .iter()
            .map(|s| {
                (
                    s.session_id.clone(),
                    (s.project.clone(), s.workspace.clone(), s.window),
                )
            })
            .collect();
        let mut changed = false;
        for s in &mut sessions {
            if s.parent.is_empty() {
                continue;
            }
            if let Some((project, workspace, window)) = parents.get(&s.parent) {
                if s.project == "unassigned" && project != "unassigned" {
                    s.project = project.clone();
                    changed = true;
                }
                if s.window.is_none() && window.is_some() {
                    s.window = *window;
                    s.workspace = workspace.clone();
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    sessions
}

fn workspace_id(endpoint: &RelationEndpoint) -> Option<u64> {
    match endpoint {
        RelationEndpoint::StableKey { kind, id } if kind == locus::keys::NIRI_WORKSPACE_ID => {
            id.parse().ok()
        }
        _ => None,
    }
}
fn project_path(record: &RelationRecord) -> Option<&Path> {
    match &record.target {
        RelationEndpoint::StableKey { kind, id } if kind == locus::keys::PROJECT_PATH => {
            Some(Path::new(id))
        }
        _ => record.metadata.get("path").map(Path::new),
    }
}
fn project_name(record: &RelationRecord) -> Option<String> {
    record
        .metadata
        .get("name")
        .filter(|name| !name.trim().is_empty())
        .cloned()
        .or_else(|| {
            project_path(record)?
                .file_name()?
                .to_str()
                .map(str::to_owned)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn most_specific_project_path_and_parent_inheritance() {
        let record = |id, path: &str, name: &str| RelationRecord {
            subject: RelationEndpoint::stable_key(locus::keys::NIRI_WORKSPACE_ID, id),
            relation: "org.rsynapse.workspace.project".to_owned(),
            target: RelationEndpoint::stable_key(locus::keys::PROJECT_PATH, path),
            metadata: HashMap::from([("name".to_owned(), name.to_owned())]),
            created_at_unix_ms: 0,
            updated_at_unix_ms: 0,
        };
        let projects = vec![
            record("1", "/repo", "repo"),
            record("2", "/repo/nested", "nested"),
        ];
        let sessions = vec![
            Session {
                session_id: "parent".to_owned(),
                cwd: "/repo/nested/src".to_owned(),
                window: Some(10),
                ..Session::default()
            },
            Session {
                parent: "parent".to_owned(),
                subagent: true,
                ..Session::default()
            },
            Session {
                cwd: "/repository".to_owned(),
                ..Session::default()
            },
        ];
        let result = attribute(sessions, &HashMap::from([(10, 2)]), &projects, &[]);
        assert_eq!(result[0].project, "nested");
        assert_eq!(result[1].project, "nested");
        assert_eq!(result[1].window, Some(10));
        assert_eq!(result[2].project, "unassigned");
    }
}
