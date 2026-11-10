//! Bar display projection of the workspace -> project association.
use data_sources::{
    Observable,
    rx::Observable as _,
    workspace_project::{self, WorkspaceProject},
};

use super::workspace::WorkspaceObject;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::widgets::bar) struct ProjectDetails {
    pub(in crate::widgets::bar) has_project: bool,
    pub(in crate::widgets::bar) name: Option<String>,
    pub(in crate::widgets::bar) display_main: Option<String>,
    pub(in crate::widgets::bar) display_secondary: Option<String>,
    pub(in crate::widgets::bar) branch: Option<String>,
    pub(in crate::widgets::bar) cwd: Option<String>,
    pub(in crate::widgets::bar) path: Option<String>,
    pub(in crate::widgets::bar) project_id: Option<String>,
}

pub(in crate::widgets::bar) fn project_details(
    workspace: WorkspaceObject,
) -> Observable<ProjectDetails> {
    workspace_project::workspace_project(workspace)
        .map(ProjectDetails::from)
        .distinct_until_changed()
        .box_it()
}

impl From<WorkspaceProject> for ProjectDetails {
    fn from(project: WorkspaceProject) -> Self {
        let resolved = project.project.as_ref();
        Self {
            has_project: project.has_project,
            name: resolved.map(|project| project.name.clone()),
            display_main: resolved.map(|project| project.name.clone()),
            display_secondary: project.branch.clone(),
            branch: project.branch,
            cwd: resolved.map(|project| project.cwd.clone()),
            path: project.path,
            project_id: project.project_id,
        }
    }
}
