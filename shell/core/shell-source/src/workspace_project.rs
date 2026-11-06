//! Reactive workspace -> project association (locus) resolved through projd.
use std::collections::HashMap;

use locus::{RelationEndpoint, RelationRecord, keys};
use proj_model::ProjectInfo;

use crate::workspace::WorkspaceObject;
use crate::{Observable, rx::Observable as _};

#[cfg(test)]
mod backpressure;
mod projd;

const WORKSPACE_PROJECT_RELATION: &str = workspace_policy::PROJECT_RELATION;

/// Resolved workspace association. `has_project` reflects locus presence;
/// `project`/`branch` fill in once projd data is available.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkspaceProject {
    pub has_project: bool,
    pub path: Option<String>,
    pub project_id: Option<String>,
    pub project: Option<ProjectInfo>,
    pub branch: Option<String>,
}

/// Live project association for one workspace.
pub fn workspace_project(workspace: WorkspaceObject) -> Observable<WorkspaceProject> {
    projd::resolve(
        crate::switch_map(workspace.id().map(workspace_subject).box_it(), |subject| {
            locus_workspace_project(subject)
        })
        .box_it(),
    )
}

/// Live project association for the focused (or named output's) workspace.
pub fn focused_workspace_project(output: Option<String>) -> Observable<WorkspaceProject> {
    crate::switch_map(
        crate::workspace::current_workspace(output),
        |workspace| match workspace {
            Some(workspace) => workspace_project(workspace),
            None => crate::once(WorkspaceProject::default()),
        },
    )
}

/// Live resolved projd project for the focused (or named output's) workspace.
pub fn focused_project(output: Option<String>) -> Observable<Option<ProjectInfo>> {
    focused_workspace_project(output)
        .map(|project| project.project)
        .distinct_until_changed()
        .box_it()
}

/// Live project ID for the focused (or named output's) workspace.
pub fn focused_project_id(output: Option<String>) -> Observable<Option<String>> {
    focused_workspace_project(output)
        .map(|project| project.project_id)
        .distinct_until_changed()
        .box_it()
}

fn locus_workspace_project(subject: RelationEndpoint) -> Observable<WorkspaceProject> {
    let filter_subject = subject.clone();
    crate::watch_relation_subject(subject, WORKSPACE_PROJECT_RELATION)
        .map(move |records| {
            records
                .into_iter()
                .find(|record| record.subject == filter_subject)
                .map(WorkspaceProject::from)
                .unwrap_or_default()
        })
        .distinct_until_changed()
        .box_it()
}

impl From<RelationRecord> for WorkspaceProject {
    fn from(record: RelationRecord) -> Self {
        let path = project_path_from_endpoint(&record.target)
            .or_else(|| metadata_value(&record.metadata, &["path"]));
        Self {
            has_project: true,
            path,
            project_id: metadata_value(&record.metadata, &["project-id"]),
            ..Self::default()
        }
    }
}

fn project_path_from_endpoint(endpoint: &RelationEndpoint) -> Option<String> {
    match endpoint {
        RelationEndpoint::StableKey { kind, id } if kind == keys::PROJECT_PATH => {
            non_empty(Some(id.clone()))
        }
        _ => None,
    }
}

fn metadata_value(metadata: &HashMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| non_empty(metadata.get(*key).cloned()))
}

fn workspace_subject(id: String) -> RelationEndpoint {
    RelationEndpoint::stable_key(keys::WORKSPACE_ID, id)
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    })
}
