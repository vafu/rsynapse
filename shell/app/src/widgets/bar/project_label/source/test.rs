use super::agent::workspace_agent_state_from_agents;
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

fn agent(state: State, attention: bool, unseen: bool) -> Agent {
    Agent {
        icon: "cognition".to_owned(),
        attention,
        state,
        context_pct: 0,
        unseen,
    }
}
