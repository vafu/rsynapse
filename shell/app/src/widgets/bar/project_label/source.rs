mod agent;
mod build;
mod workspace_icon;

#[cfg(test)]
mod test;

use shell_core::source::{rx::Observable as _, Observable};
use shell_rx_macros::combine_latest;

use self::{
    agent::{workspace_agent_state, WorkspaceAgentState},
    build::workspace_build_state,
    workspace_icon::{
        clear_workspace_icon_override, set_workspace_icon_override, workspace_icon_source,
    },
};
pub(super) use self::{
    build::WorkspaceBuildState,
    workspace_icon::{WorkspaceIconCandidate, WorkspaceIconChoice},
};
use crate::widgets::bar::{niri::NiriWorkspace, project::project_details};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::widgets::bar) struct ProjectLabelVm {
    pub(super) workspace_id: Option<u64>,
    pub(super) index: u32,
    pub(super) workspace_name: String,
    pub(super) urgent: bool,
    pub(super) active: bool,
    pub(super) project_name: Option<String>,
    pub(super) project_branch: Option<String>,
    pub(super) project_icon_glyph: String,
    pub(super) project_icon_input: String,
    pub(super) project_icon_candidates: Vec<WorkspaceIconCandidate>,
    pub(super) project_icon_overridden: bool,
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
        workspace.id().map(Some),
        workspace.index().map(u32::from),
        workspace.name().map(|name| name.unwrap_or_default()),
        workspace.urgent(),
        workspace.focused(),
        project,
        workspace_icon,
        agent,
        build
            => |(workspace_id, index, workspace_name, urgent, active, project, workspace_icon, agent, build)| {
                ProjectLabelVm {
                    workspace_id,
                    index,
                    workspace_name,
                    urgent,
                    active,
                    project_name: project.display_main,
                    project_branch: project.display_secondary,
                    project_icon_glyph: workspace_icon.glyph,
                    project_icon_input: workspace_icon.picker_input,
                    project_icon_candidates: workspace_icon.candidates,
                    project_icon_overridden: workspace_icon.overridden,
                    empty: workspace_icon.empty,
                    agent,
                    build,
                }
            },
    )
    .distinct_until_changed()
    .box_it()
}

pub(super) fn set_project_icon_override(
    workspace_id: Option<u64>,
    icon: WorkspaceIconChoice,
    picker_input: String,
) {
    let Some(workspace_id) = workspace_id else {
        eprintln!("[project-label] cannot set icon override without workspace id");
        return;
    };
    set_workspace_icon_override(workspace_id, icon, picker_input);
}

pub(super) fn clear_project_icon_override(workspace_id: Option<u64>) {
    let Some(workspace_id) = workspace_id else {
        eprintln!("[project-label] cannot clear icon override without workspace id");
        return;
    };
    clear_workspace_icon_override(workspace_id);
}
