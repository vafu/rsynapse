mod agent;
mod project;
mod workspace_fallback;

#[cfg(test)]
mod test;

use shell_core::source::{Observable, rx::Observable as _};
use shell_rx_macros::combine_latest;

use self::{
    agent::{WorkspaceAgentState, workspace_agent_state},
    project::project_details,
    workspace_fallback::workspace_window_fallback_source,
};
use crate::widgets::bar::niri::NiriWorkspace;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::widgets::bar) struct ProjectLabelVm {
    pub(super) index: u32,
    pub(super) workspace_name: String,
    pub(super) urgent: bool,
    pub(super) active: bool,
    pub(super) project_name: Option<String>,
    pub(super) project_branch: Option<String>,
    pub(super) project_icon: Option<String>,
    pub(super) project_icon_is_app: bool,
    pub(super) empty: bool,
    pub(super) agent: WorkspaceAgentState,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ProjectDetails {
    has_project: bool,
    name: Option<String>,
    branch: Option<String>,
    icon: Option<String>,
}

pub(super) fn project_label_vm(workspace: NiriWorkspace) -> Observable<ProjectLabelVm> {
    let project = project_details(workspace.clone());
    let workspace_fallback = workspace_window_fallback_source(workspace.clone());
    let agent = workspace_agent_state(workspace.clone());

    combine_latest!(
        workspace.index().map(u32::from),
        workspace.name().map(|name| name.unwrap_or_default()),
        workspace.urgent(),
        workspace.focused(),
        project,
        workspace_fallback,
        agent
            => |(index, workspace_name, urgent, active, project, fallback, agent)| {
                let fallback_icon = (!project.has_project).then_some(fallback.icon).flatten();
                let project_icon_is_app = fallback_icon.is_some();
                let project_icon = project.icon.or(fallback_icon);
                ProjectLabelVm {
                    index,
                    workspace_name,
                    urgent,
                    active,
                    project_name: project.name,
                    project_branch: project.branch,
                    project_icon,
                    project_icon_is_app,
                    empty: !project.has_project && fallback.empty,
                    agent,
                }
            },
    )
    .distinct_until_changed()
    .box_it()
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    })
}
