use super::super::{Agent, State};
use super::actual::{AgentSeenState, agent_icon, agent_with_seen_state, session_state};

#[test]
fn maps_agent_name_to_renderable_icon() {
    assert_eq!(agent_icon("codex", "", ""), "cognition");
    assert_eq!(agent_icon(" Codex ", "ignored", "ignored"), "cognition");
}

#[test]
fn ignores_arbitrary_metadata_as_icon_names() {
    assert_eq!(
        agent_icon("unknown-agent", "not an icon", "reviewer"),
        "cognition"
    );
    assert_eq!(agent_icon("", "", ""), "cognition");
}

#[test]
fn maps_agent_dbus_state() {
    assert_eq!(session_state("idle"), State::Idle);
    assert_eq!(session_state(" thinking "), State::Thinking);
    assert_eq!(session_state("tool-use"), State::ToolUse);
    assert_eq!(session_state("compacting"), State::Compacting);
    assert_eq!(session_state(""), State::None);
}

#[test]
fn marks_agent_unseen_when_work_finishes_out_of_focus() {
    let mut seen = AgentSeenState::default();

    let updated =
        agent_with_seen_state(Some(make_agent(State::Thinking)), false, &mut seen).expect("agent");
    assert!(!updated.unseen);

    let updated =
        agent_with_seen_state(Some(make_agent(State::Idle)), false, &mut seen).expect("agent");
    assert!(updated.unseen);
}

#[test]
fn keeps_agent_seen_when_work_finishes_in_focus() {
    let mut seen = AgentSeenState::default();

    let updated =
        agent_with_seen_state(Some(make_agent(State::Thinking)), true, &mut seen).expect("agent");
    assert!(!updated.unseen);

    let updated =
        agent_with_seen_state(Some(make_agent(State::Idle)), true, &mut seen).expect("agent");
    assert!(!updated.unseen);
}

#[test]
fn clears_unseen_agent_when_window_is_selected() {
    let mut seen = AgentSeenState::default();

    let updated =
        agent_with_seen_state(Some(make_agent(State::Idle)), false, &mut seen).expect("agent");
    assert!(updated.unseen);

    let updated =
        agent_with_seen_state(Some(make_agent(State::Idle)), true, &mut seen).expect("agent");
    assert!(!updated.unseen);
}

#[test]
fn clears_unseen_agent_when_agent_starts_working_again() {
    let mut seen = AgentSeenState::default();

    let updated =
        agent_with_seen_state(Some(make_agent(State::Idle)), false, &mut seen).expect("agent");
    assert!(updated.unseen);

    let updated =
        agent_with_seen_state(Some(make_agent(State::ToolUse)), false, &mut seen).expect("agent");
    assert!(!updated.unseen);
}

#[test]
fn clears_unseen_agent_when_state_is_unknown() {
    let mut seen = AgentSeenState::default();

    let updated =
        agent_with_seen_state(Some(make_agent(State::Idle)), false, &mut seen).expect("agent");
    assert!(updated.unseen);

    let updated =
        agent_with_seen_state(Some(make_agent(State::None)), false, &mut seen).expect("agent");
    assert!(!updated.unseen);
}

fn make_agent(state: State) -> Agent {
    Agent {
        icon: "cognition".to_owned(),
        attention: false,
        state,
        context_pct: 0,
        unseen: false,
    }
}
