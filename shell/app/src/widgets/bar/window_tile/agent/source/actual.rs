use futures_util::StreamExt;
use shell_core::source::{
    self, Observable,
    dbus::{
        self, Bus, DbusInterface, DbusObject, ObjectDescriptor, ObjectManagerDescriptor,
        PropertyDescriptor,
    },
    rx::Observable as _,
};
use shell_rx_macros::combine_latest;
use zbus::zvariant::OwnedObjectPath;

use super::super::{Agent, State};
use crate::widgets::bar::WindowNode;

const AGENT_DBUS_BUS: &str = "io.github.AgentDBus";
const AGENT_DBUS_ROOT_PATH: &str = "/io/github/AgentDBus";
const AGENT_SESSION_INTERFACE: &str = "io.github.AgentDBus1.Session";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AgentSession {
    pub(super) path: OwnedObjectPath,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct AgentSeenState {
    last_agent_state: Option<State>,
    unseen: bool,
}

pub(super) fn agent_for_window(window: WindowNode) -> Observable<Option<Agent>> {
    let Some(window_id) = window.path_id() else {
        return source::once(None);
    };

    source::shared_by_key(
        "rsynapse.agent-for-window",
        window_id.to_string(),
        move || agent_for_window_status(window_id, window.clone()),
    )
    .distinct_until_changed()
    .box_it()
}

fn agent_for_window_status(window_id: u64, window: WindowNode) -> Observable<Option<Agent>> {
    source::from_task(move |sender| {
        let window = window.clone();
        async move {
            let agent = raw_agent_for_window(window_id);
            let focused = window.focused();
            run_agent_seen_state(sender, agent, focused).await;
        }
    })
    .distinct_until_changed()
    .box_it()
}

fn raw_agent_for_window(window_id: u64) -> Observable<Option<Agent>> {
    source::switch_map(agent_session_for_window_id(window_id), |session| {
        session
            .map(agent_session)
            .unwrap_or_else(|| source::once(None))
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

fn agent_session_for_window_id(window_id: u64) -> Observable<Option<AgentSession>> {
    // AgentDBus is authoritative for live window ownership. Locus window-agent
    // relations can outlive niri's live window ids and make reused ids inherit
    // stale agent state.
    dbus::object_manager(agent_dbus())
        .map(move |objects| find_agent_session_by_window_id(&objects, window_id))
        .distinct_until_changed()
        .box_it()
}

pub(super) fn find_agent_session_by_window_id(
    objects: &[DbusObject],
    window_id: u64,
) -> Option<AgentSession> {
    let window_id = window_id.to_string();
    objects
        .iter()
        .filter(|object| has_interface(object, AGENT_SESSION_INTERFACE))
        .find(|object| {
            snapshot_property::<String>(object, AGENT_SESSION_INTERFACE, "WindowId").as_deref()
                == Some(window_id.as_str())
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

fn agent_dbus() -> ObjectManagerDescriptor {
    ObjectManagerDescriptor::parse(Bus::Session, AGENT_DBUS_BUS, AGENT_DBUS_ROOT_PATH)
        .expect("AgentDBus descriptor should be valid")
}

fn has_interface(object: &DbusObject, interface_name: &str) -> bool {
    interface(object, interface_name).is_some()
}

fn snapshot_property<T>(object: &DbusObject, interface_name: &str, property_name: &str) -> Option<T>
where
    T: TryFrom<zbus::zvariant::OwnedValue>,
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
    combine_latest!(
        session.agent_name(),
        session.agent_nickname(),
        session.agent_role(),
        session.state(),
        session.requires_attention(),
        session.cwd(),
        session.session_title()
            => |(agent_name, nickname, role, state, attention, cwd, title)| {
                Some(Agent {
                    name: agent_name.clone(),
                    icon: agent_icon(&agent_name, &nickname, &role),
                    cwd,
                    title,
                    attention,
                    state: session_state(&state),
                    unseen: false,
                })
            },
    )
    .distinct_until_changed()
    .box_it()
}

impl AgentSession {
    fn agent_name(&self) -> Observable<String> {
        required(self.property("AgentName"), String::new())
    }

    fn agent_nickname(&self) -> Observable<String> {
        required(self.property("AgentNickname"), String::new())
    }

    fn agent_role(&self) -> Observable<String> {
        required(self.property("AgentRole"), String::new())
    }

    fn state(&self) -> Observable<String> {
        required(self.property("State"), String::new())
    }

    fn requires_attention(&self) -> Observable<bool> {
        required(self.property("RequiresAttention"), false)
    }

    fn cwd(&self) -> Observable<String> {
        required(self.property("Cwd"), String::new())
    }

    fn session_title(&self) -> Observable<String> {
        required(self.property("SessionTitle"), String::new())
    }

    fn property(&self, name: &'static str) -> PropertyDescriptor {
        PropertyDescriptor::new(agent_session_object(self.path.as_str()), name)
    }
}

fn agent_session_object(path: &str) -> ObjectDescriptor {
    ObjectDescriptor::parse(Bus::Session, AGENT_DBUS_BUS, path, AGENT_SESSION_INTERFACE)
        .expect("static AgentDBus descriptor should be valid")
}

fn required<T>(descriptor: PropertyDescriptor, default: T) -> Observable<T>
where
    T: TryFrom<zbus::zvariant::OwnedValue> + Clone + PartialEq + Send + 'static,
    T::Error: std::fmt::Display,
{
    dbus::property_or(descriptor, default)
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
