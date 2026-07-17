use super::agent::workspace_agent_state_from_agents;
use super::build::{WorkspaceBuildState, workspace_build_state_from_builds};
use crate::widgets::bar::bzbus::BzBusView;
use crate::widgets::bar::window_tile::agent::{Agent, State};

#[test]
fn workspace_agent_state_tracks_unseen_agents() {
    let state = workspace_agent_state_from_agents(vec![Some(agent(State::Idle, false, true))]);

    assert!(state.has_unseen);
    assert!(!state.has_working);
    assert!(!state.has_attention);
}

#[test]
fn workspace_agent_state_keeps_working_and_attention_precedence() {
    let state = workspace_agent_state_from_agents(vec![
        Some(agent(State::Idle, false, true)),
        Some(agent(State::ToolUse, false, false)),
        Some(agent(State::None, true, false)),
    ]);

    assert!(state.has_unseen);
    assert!(state.has_working);
    assert!(state.has_attention);
}

#[test]
fn workspace_build_state_uses_failed_running_finished_precedence() {
    assert_eq!(
        workspace_build_state_from_builds(vec![Some(build("running")), Some(build("failed"))]),
        WorkspaceBuildState::Failed
    );
    assert_eq!(
        workspace_build_state_from_builds(vec![Some(build("finished")), Some(build("running"))]),
        WorkspaceBuildState::Running
    );
    assert_eq!(
        workspace_build_state_from_builds(vec![Some(build("finished")), Some(build("finished"))]),
        WorkspaceBuildState::Finished
    );
    assert_eq!(
        workspace_build_state_from_builds(vec![Some(build("idle"))]),
        WorkspaceBuildState::None
    );
}

fn agent(state: State, attention: bool, unseen: bool) -> Agent {
    Agent {
        icon: "cognition".to_owned(),
        attention,
        state,
        context_pct: 0,
        unseen,
    }
}

fn build(state: &'static str) -> BzBusView {
    BzBusView {
        classes: vec!["barblock", "bzbus-widget", state],
        tooltip: String::new(),
        icon: "",
        progress_level_classes: vec![],
        progress_percent: 0,
        progress_visible: false,
    }
}
