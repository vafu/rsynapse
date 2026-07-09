use std::sync::Arc;

use shell_core::source::dbus::{DbusInterface, DbusObject, DbusPropertyValue};
use zbus::{
    names::OwnedInterfaceName,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

use super::super::{Agent, State};
use super::actual::{
    AgentSeenState, agent_icon, agent_with_seen_state, find_agent_session_by_window_id,
    session_state,
};

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
fn finds_agent_session_by_window_id_snapshot() {
    let objects = vec![session_object(
        "/io/github/AgentDBus/sessions/codex/session",
        "42",
    )];

    let session = find_agent_session_by_window_id(&objects, 42).expect("session");

    assert_eq!(
        session.path.as_str(),
        "/io/github/AgentDBus/sessions/codex/session"
    );
    assert!(find_agent_session_by_window_id(&objects, 41).is_none());
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

fn session_object(path: &str, window_id: &str) -> DbusObject {
    DbusObject {
        path: OwnedObjectPath::try_from(path).unwrap(),
        interfaces: vec![DbusInterface {
            name: OwnedInterfaceName::try_from("io.github.AgentDBus1.Session").unwrap(),
            properties: vec![DbusPropertyValue {
                name: "WindowId".to_owned(),
                value: Arc::new(OwnedValue::try_from(Value::from(window_id)).unwrap()),
            }],
        }],
    }
}
