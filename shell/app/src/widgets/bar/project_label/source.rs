mod agent;
mod build;
mod workspace_icon;

#[cfg(test)]
mod test;

use shell_core::source::{Observable, rx::Observable as _};
use shell_rx_macros::combine_latest;

pub(super) use self::build::WorkspaceBuildState;
use self::{
    agent::{WorkspaceAgentState, workspace_agent_state},
    build::workspace_build_state,
    workspace_icon::workspace_icon_source,
};
use crate::widgets::bar::{niri::NiriWorkspace, project::project_details};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::widgets::bar) struct ProjectLabelVm {
    pub(super) index: u32,
    pub(super) workspace_name: String,
    pub(super) urgent: bool,
    pub(super) active: bool,
    pub(super) project_name: Option<String>,
    pub(super) project_branch: Option<String>,
    pub(super) project_icon: String,
    pub(super) project_icon_input: String,
    pub(super) empty: bool,
    pub(super) agent: WorkspaceAgentState,
    pub(super) build: WorkspaceBuildState,
}

pub(super) fn project_label_vm(workspace: NiriWorkspace) -> Observable<ProjectLabelVm> {
    let project = project_details(workspace.clone());
    let workspace_icon = workspace_icon_source(workspace.clone());
    let agent = workspace_agent_state(workspace.clone());
    let build = workspace_build_state(workspace.clone());

    combine_latest!(
        workspace.index().map(u32::from),
        workspace.name().map(|name| name.unwrap_or_default()),
        workspace.urgent(),
        workspace.focused(),
        project,
        workspace_icon,
        agent,
        build
            => |(index, workspace_name, urgent, active, project, workspace_icon, agent, build)| {
                ProjectLabelVm {
                    index,
                    workspace_name,
                    urgent,
                    active,
                    project_name: project.display_main,
                    project_branch: project.display_secondary,
                    project_icon: workspace_icon.icon,
                    project_icon_input: workspace_icon.picker_input,
                    empty: workspace_icon.empty,
                    agent,
                    build,
                }
            },
    )
    .distinct_until_changed()
    .box_it()
}
