use shell_core::source::{self, Observable, rx::Observable as _};

use crate::widgets::bar::{
    niri::NiriWorkspace,
    window_source::{WindowSnapshot, window_snapshots},
    window_tile::agent::{Agent, State, agent_for_window},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::widgets::bar::project_label) struct WorkspaceAgentState {
    pub(in crate::widgets::bar::project_label) has_attention: bool,
    pub(in crate::widgets::bar::project_label) has_working: bool,
}

pub(super) fn workspace_agent_state(workspace: NiriWorkspace) -> Observable<WorkspaceAgentState> {
    source::switch_map(
        workspace
            .id()
            .map(Some)
            .combine_latest(window_snapshots(), workspace_windows)
            .distinct_until_changed()
            .box_it(),
        |windows| {
            source::switch_map_list(source::once(windows), window_agent_state)
                .map(workspace_agent_state_from_agents)
                .distinct_until_changed()
                .box_it()
        },
    )
    .distinct_until_changed()
    .box_it()
}

fn workspace_windows(
    workspace_id: Option<u64>,
    windows: Vec<WindowSnapshot>,
) -> Vec<WindowSnapshot> {
    windows
        .into_iter()
        .filter(|window| window.workspace_id == workspace_id)
        .collect()
}

fn window_agent_state(window: WindowSnapshot) -> Observable<Option<Agent>> {
    agent_for_window(window.window)
}

fn workspace_agent_state_from_agents(agents: Vec<Option<Agent>>) -> WorkspaceAgentState {
    WorkspaceAgentState {
        has_attention: agents.iter().flatten().any(|agent| agent.attention),
        has_working: agents.iter().flatten().any(|agent| {
            matches!(
                agent.state,
                State::Thinking | State::ToolUse | State::Compacting
            )
        }),
    }
}
