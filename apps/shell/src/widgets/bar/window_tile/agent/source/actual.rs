use data_sources::{
    self, Observable,
    dbus::{self, Bus, DbusInterface, DbusObject, ObjectManagerDescriptor},
    rx::Observable as _,
};
use futures_util::StreamExt;
use shell_rx_macros::combine_latest;

use super::super::{Agent, State};
use super::{
    AGENT_DBUS_BUS, AGENT_DBUS_ROOT_PATH, AGENT_SESSION_INTERFACE, properties::AgentSession,
};
use crate::widgets::bar::WindowNode;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct AgentSeenState {
    last_agent_state: Option<State>,
    unseen: bool,
}

pub(super) fn agent_for_window(window: WindowNode) -> Observable<Option<Agent>> {
    data_sources::switch_map(window.id(), move |window_id| {
        let window = window.clone();
        data_sources::shared_by_key("rsynapse.agent-for-window", window_id.clone(), move || {
            agent_for_window_status(window_id.clone(), window.clone())
        })
    })
    .distinct_until_changed()
    .box_it()
}

fn agent_for_window_status(window_id: String, window: WindowNode) -> Observable<Option<Agent>> {
    data_sources::from_task(move |sender| {
        let window = window.clone();
        let window_id = window_id.clone();
        async move {
            let agent = raw_agent_for_window(window_id);
            let focused = window.focused();
            run_agent_seen_state(sender, agent, focused).await;
        }
    })
    .distinct_until_changed()
    .box_it()
}

pub(super) fn raw_agent_for_window(window_id: String) -> Observable<Option<Agent>> {
    data_sources::switch_map(agent_session_for_window_id(window_id), |session| {
        session
            .map(agent_session)
            .unwrap_or_else(|| data_sources::once(None))
    })
    .distinct_until_changed()
    .box_it()
}

async fn run_agent_seen_state(
    sender: async_channel::Sender<Result<Option<Agent>, String>>,
    agent: Observable<Option<Agent>>,
    focused: Observable<bool>,
) {
    let mut updates = Box::pin(
        agent
            .combine_latest(focused, |agent, focused| (agent, focused))
            .into_stream(),
    );
    let mut state = AgentSeenState::default();

    while let Some(update) = updates.next().await {
        let value = match update {
            Ok((agent, focused)) => Ok(agent_with_seen_state(agent, focused, &mut state)),
            Err(error) => Err(error),
        };

        if sender.send(value).await.is_err() {
            return;
        }
    }
}

fn agent_session_for_window_id(window_id: String) -> Observable<Option<AgentSession>> {
    // AgentDBus is authoritative for live window ownership. Locus window-agent
    // relations can outlive compositor window IDs and make reused IDs inherit
    // stale agent state.
    dbus::object_manager(agent_dbus())
        .map(move |objects| find_agent_session_by_window_id(&objects, &window_id))
        .distinct_until_changed()
        .box_it()
}

pub(super) fn find_agent_session_by_window_id(
    objects: &[DbusObject],
    window_id: &str,
) -> Option<AgentSession> {
    objects
        .iter()
        .filter(|object| has_interface(object, AGENT_SESSION_INTERFACE))
        .find(|object| {
            snapshot_property::<String>(object, AGENT_SESSION_INTERFACE, "WindowId").as_deref()
                == Some(window_id)
        })
        .map(|object| AgentSession {
            path: object.path.clone(),
        })
}

pub(super) fn agent_with_seen_state(
    mut agent: Option<Agent>,
    focused: bool,
    state: &mut AgentSeenState,
) -> Option<Agent> {
    let agent_state = agent.as_ref().map(|agent| agent.state);

    match agent_state {
        Some(State::Idle) if state.last_agent_state != Some(State::Idle) => {
            state.unseen = !focused;
        }
        Some(State::Thinking | State::ToolUse | State::Compacting | State::None) | None => {
            state.unseen = false;
        }
        Some(State::Idle) => {}
    }

    if focused {
        state.unseen = false;
    }

    state.last_agent_state = agent_state;

    if let Some(agent) = agent.as_mut() {
        agent.unseen = state.unseen;
    }
    agent
}

/// Live count of subagent sessions parented to `session_id`.
///
/// Membership comes from the ObjectManager snapshot, which is exactly the
/// right granularity here: subagent sessions are added when spawned and
/// removed when they stop, unlike pending-request state which changes via
/// properties on long-lived objects.
fn subagent_count(session_id: &str) -> Observable<usize> {
    let key = session_id.to_owned();
    data_sources::shared_by_key("rsynapse.agent-subagent-count", key.clone(), move || {
        let key = key.clone();
        dbus::object_manager(agent_dbus())
            .map(move |objects| count_subagents(&objects, &key))
            .distinct_until_changed()
            .box_it()
    })
}

pub(super) fn count_subagents(objects: &[DbusObject], parent_session_id: &str) -> usize {
    if parent_session_id.is_empty() {
        return 0;
    }
    objects
        .iter()
        .filter(|object| has_interface(object, AGENT_SESSION_INTERFACE))
        .filter(|object| {
            snapshot_property::<String>(object, AGENT_SESSION_INTERFACE, "ParentSessionId")
                .as_deref()
                == Some(parent_session_id)
        })
        .count()
}

fn agent_dbus() -> ObjectManagerDescriptor {
    ObjectManagerDescriptor::parse(Bus::Session, AGENT_DBUS_BUS, AGENT_DBUS_ROOT_PATH)
        .expect("AgentDBus descriptor should be valid")
}

fn has_interface(object: &DbusObject, interface_name: &str) -> bool {
    interface(object, interface_name).is_some()
}

fn snapshot_property<T>(object: &DbusObject, interface_name: &str, property_name: &str) -> Option<T>
where
    T: TryFrom<data_sources::zvariant::OwnedValue>,
    T::Error: std::fmt::Display,
{
    let property = interface(object, interface_name)?
        .properties
        .iter()
        .find(|property| property.name == property_name)?;
    let value = property.value.as_ref().try_clone().ok()?;
    T::try_from(value).ok()
}

fn interface<'a>(object: &'a DbusObject, interface_name: &str) -> Option<&'a DbusInterface> {
    object
        .interfaces
        .iter()
        .find(|interface| interface.name.as_str() == interface_name)
}

fn agent_session(session: AgentSession) -> Observable<Option<Agent>> {
    let context_pct = session.context_pct();
    let base = combine_latest!(
        session.agent_name(),
        session.agent_nickname(),
        session.agent_role(),
        session.state(),
        session.requires_attention(),
        session.cwd(),
        session.session_title(),
        session.session_id()
            => |(agent_name, nickname, role, state, attention, cwd, title, session_id)| {
                Some((
                    Agent {
                        name: agent_name.clone(),
                        icon: agent_icon(&agent_name, &nickname, &role),
                        cwd,
                        title,
                        attention,
                        state: session_state(&state),
                        unseen: false,
                        subagents: 0,
                        context_pct: None,
                    },
                    session_id,
                ))
            },
    )
    .distinct_until_changed()
    .box_it();
    data_sources::switch_map(base, |agent| match agent {
        Some((agent, session_id)) => subagent_count(&session_id)
            .map(move |subagents| {
                let mut agent = agent.clone();
                agent.subagents = subagents;
                Some(agent)
            })
            .distinct_until_changed()
            .box_it(),
        None => data_sources::once(None),
    })
    .combine_latest(context_pct, |agent, context_pct| {
        agent.map(|mut agent| {
            agent.context_pct = context_pct;
            agent
        })
    })
    .distinct_until_changed()
    .box_it()
}

pub(super) fn agent_icon(_agent_name: &str, _nickname: &str, _role: &str) -> String {
    "cognition".to_string()
}

pub(super) fn session_state(state: &str) -> State {
    match state.trim() {
        "idle" => State::Idle,
        "thinking" => State::Thinking,
        "tool-use" => State::ToolUse,
        "compacting" => State::Compacting,
        _ => State::None,
    }
}
