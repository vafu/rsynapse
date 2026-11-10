use data_sources::{Observable, proj::ProjectInfo, rx::Observable as _};
use locus::{RelationEndpoint, RelationRecord};
use shell_rx_macros::combine_latest;

use crate::relations::{self, LocusClient};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Context {
    pub workspace_id: Option<String>,
    pub project_id: Option<String>,
    pub project: Option<String>,
    pub workspace_name: Option<String>,
}

pub(super) fn source(locus: LocusClient) -> Observable<Context> {
    combine_latest!(
        crate::workspace::focused_workspace_id(),
        relations::records(locus.clone(), relations::WORKSPACE_PROJECT),
        relations::records(locus, relations::WORKSPACE_NAME),
        // Metadata enrichment must not hold workspace identity/manual names.
        data_sources::proj::projects().start_with(vec![Vec::new()])
        => move |(workspace, bindings, names, projects): (Option<String>, Vec<RelationRecord>, Vec<RelationRecord>, Vec<ProjectInfo>)| {
            workspace.map(|workspace| resolve(&workspace, &bindings, &names, &projects)).unwrap_or_default()
        },
    )
    .distinct_until_changed()
    .box_it()
}

fn text(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn resolve(
    workspace: &str,
    bindings: &[RelationRecord],
    names: &[RelationRecord],
    projects: &[ProjectInfo],
) -> Context {
    let subject = RelationEndpoint::stable_key(locus::keys::WORKSPACE_ID, workspace);
    let binding = bindings.iter().find(|row| row.subject == subject);
    let project_id = binding
        .and_then(|row| row.metadata.get("project-id"))
        .and_then(|id| text(id));
    let project = project_id
        .as_ref()
        .and_then(|id| projects.iter().find(|p| &p.id == id))
        .and_then(|project| text(&project.name));
    let manual = names
        .iter()
        .find(|row| row.subject == subject)
        .filter(|row| row.metadata.get("source").map(String::as_str) == Some("manual"))
        .and_then(|row| match &row.target {
            RelationEndpoint::StableKey { kind, id } if kind == relations::WORKSPACE_NAME_KIND => {
                text(id)
            }
            _ => None,
        });
    let workspace_name = manual
        .or_else(|| project.clone())
        .or_else(|| binding.is_none().then(|| "empty".to_owned()));
    Context {
        workspace_id: Some(workspace.to_owned()),
        project_id,
        project,
        workspace_name,
    }
}

#[cfg(test)]
#[path = "context/test.rs"]
pub(super) mod test;
