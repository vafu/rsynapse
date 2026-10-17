//! Preserve workspace project intent through live window identities when a
//! compositor replaces workspace objects during monitor evacuation/hotplug.
use std::collections::{HashMap, HashSet};

use locus::{RelationEndpoint, RelationRecord};

use super::{LocusClient, WINDOW_WORKSPACE_PROJECT, WORKSPACE_PROJECT_POLICY, workspace_cleanup};
use crate::relations::WORKSPACE_PROJECT;

const OWNER: &str = "workspace-source-owner";
const GENERATION: &str = "workspace-source-generation";

fn key<'a>(endpoint: &'a RelationEndpoint, expected: &str) -> Option<&'a str> {
    match endpoint {
        RelationEndpoint::StableKey { kind, id } if kind == expected => Some(id),
        _ => None,
    }
}

fn current(metadata: &HashMap<String, String>, source: &workspace_cleanup::Snapshot) -> bool {
    metadata.get(OWNER) == Some(&source.owner)
        && metadata.get(GENERATION) == Some(&source.generation.to_string())
}

#[derive(Debug, PartialEq)]
enum Change {
    Remember {
        window: String,
        target: RelationEndpoint,
        metadata: HashMap<String, String>,
    },
    Forget(RelationEndpoint, RelationEndpoint),
    Restore {
        workspace: String,
        path: String,
        project: String,
    },
}

fn plan(source: &workspace_cleanup::Snapshot, records: &[RelationRecord]) -> Vec<Change> {
    let anchors: Vec<_> = records
        .iter()
        .filter(|record| {
            record.relation == WINDOW_WORKSPACE_PROJECT
                && record
                    .metadata
                    .get("managed-by")
                    .is_some_and(|value| value == "rsynapse-steward")
        })
        .collect();
    let bindings: HashMap<_, _> = records
        .iter()
        .filter(|r| r.relation == WORKSPACE_PROJECT)
        .filter_map(|r| Some((key(&r.subject, locus::keys::WORKSPACE_ID)?, r)))
        .collect();
    let blocked: HashSet<_> = records
        .iter()
        .filter(|r| {
            r.relation == WORKSPACE_PROJECT_POLICY
                && r.metadata
                    .get("automatic")
                    .is_some_and(|value| value == "false")
        })
        .filter_map(|r| key(&r.subject, locus::keys::WORKSPACE_ID))
        .collect();
    let mut changes = Vec::new();
    let mut candidates = HashMap::<String, HashMap<String, &RelationRecord>>::new();
    for anchor in &anchors {
        let Some(window) = key(&anchor.subject, locus::keys::WINDOW_ID) else {
            continue;
        };
        if !source.connected
            || !current(&anchor.metadata, source)
            || !source.windows.contains_key(window)
        {
            changes.push(Change::Forget(
                anchor.subject.clone(),
                anchor.target.clone(),
            ));
        }
    }
    if !source.connected {
        return changes;
    }
    for (window, workspace) in &source.windows {
        let Some(workspace) = workspace else { continue };
        let subject = RelationEndpoint::stable_key(locus::keys::WINDOW_ID, window);
        let prior = anchors.iter().find(|anchor| anchor.subject == subject);
        if blocked.contains(workspace.as_str()) {
            if let Some(anchor) = prior {
                changes.push(Change::Forget(
                    anchor.subject.clone(),
                    anchor.target.clone(),
                ));
            }
        } else if let Some(binding) = bindings.get(workspace.as_str()) {
            if current(&binding.metadata, source)
                && binding.metadata.contains_key("project-id")
                && prior.is_none_or(|anchor| {
                    anchor.target != binding.target || anchor.metadata != binding.metadata
                })
            {
                changes.push(Change::Remember {
                    window: window.clone(),
                    target: binding.target.clone(),
                    metadata: binding.metadata.clone(),
                });
            }
        } else {
            for anchor in anchors
                .iter()
                .filter(|anchor| anchor.subject == subject && current(&anchor.metadata, source))
            {
                if let Some(project) = anchor.metadata.get("project-id") {
                    candidates
                        .entry(workspace.clone())
                        .or_default()
                        .insert(project.clone(), anchor);
                }
            }
        }
    }
    for (workspace, projects) in candidates {
        // A monitor evacuation may merge windows from different projects into
        // one workspace. Preserve that ambiguity for explicit user selection.
        if projects.len() == 1 {
            let (project, anchor) = projects.into_iter().next().unwrap();
            if let Some(path) = key(&anchor.target, locus::keys::PROJECT_PATH) {
                changes.push(Change::Restore {
                    workspace,
                    path: path.into(),
                    project,
                });
            }
        }
    }
    changes
}

pub(super) async fn remember(
    locus: &LocusClient,
    workspace: &str,
    target: &RelationEndpoint,
    metadata: &HashMap<String, String>,
) -> anyhow::Result<()> {
    let connection = zbus::Connection::session().await?;
    let Some(source) = workspace_cleanup::snapshot(&connection).await? else {
        return Ok(());
    };
    if !source.connected || !current(metadata, &source) {
        return Ok(());
    }
    for (window, member) in source.windows {
        if member.as_deref() == Some(workspace) {
            locus
                .set_one_with_persistence(
                    RelationEndpoint::stable_key(locus::keys::WINDOW_ID, window),
                    WINDOW_WORKSPACE_PROJECT,
                    target.clone(),
                    metadata.clone(),
                    false,
                )
                .await?;
        }
    }
    Ok(())
}

pub(super) async fn reconcile(
    locus: &LocusClient,
    source: &workspace_cleanup::Snapshot,
) -> anyhow::Result<()> {
    let records = locus.list("").await?;
    for change in plan(source, &records) {
        match change {
            Change::Remember {
                window,
                target,
                metadata,
            } => {
                locus
                    .set_one_with_persistence(
                        RelationEndpoint::stable_key(locus::keys::WINDOW_ID, window),
                        WINDOW_WORKSPACE_PROJECT,
                        target,
                        metadata,
                        false,
                    )
                    .await?;
            }
            Change::Forget(subject, target) => {
                locus
                    .unset(subject, WINDOW_WORKSPACE_PROJECT, target)
                    .await?
            }
            Change::Restore {
                workspace,
                path,
                project,
            } => {
                let result = super::proj()
                    .await?
                    .call::<_, _, goal_model::ProjectInfo>("ResolveProject", &(path.clone(),))
                    .await;
                let context = match result {
                    Ok(context) => context,
                    Err(error) => {
                        eprintln!("[steward/workspace-recovery] cannot resolve {path}: {error}");
                        continue;
                    }
                };
                if context.id == project {
                    super::bind_context(locus, context, Some(workspace), None, false).await?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
