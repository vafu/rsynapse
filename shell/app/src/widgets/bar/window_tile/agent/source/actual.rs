use futures_util::StreamExt;
use locus::{RelationEndpoint, RelationRecord, keys};
use shell_core::source::{
    self, Observable,
    dbus::{
        self, Bus, DbusInterface, DbusObject, ObjectDescriptor, ObjectManagerDescriptor,
        PropertyDescriptor,
    },
    rx::Observable as _,
};
use shell_rx_macros::combine_latest;
use zbus::{Connection, Proxy, zvariant::OwnedObjectPath};

use super::super::{Agent, State};
use crate::widgets::bar::WindowNode;

const AGENT_DBUS_BUS: &str = "io.github.AgentDBus";
const AGENT_DBUS_ROOT_PATH: &str = "/io/github/AgentDBus";
const AGENT_SESSION_INTERFACE: &str = "io.github.AgentDBus1.Session";
const AGENT_SESSION_PREFIX: &str = "/io/github/AgentDBus/sessions/";
const WINDOW_AGENT_RELATION: &str = "org.rsynapse.window.agent-session";

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
    let keyed_window = window.clone();
    source::switch_map(window.id().box_it(), move |window_id| {
        let window = keyed_window.clone();
        source::shared_by_key(
            "rsynapse.agent-for-window",
            window_id.to_string(),
            move || agent_for_window_status(window.clone()),
        )
    })
    .distinct_until_changed()
    .box_it()
}

fn agent_for_window_status(window: WindowNode) -> Observable<Option<Agent>> {
    source::from_task(move |sender| {
        let window = window.clone();
        async move {
            let agent = raw_agent_for_window(window.clone());
            let focused = window.focused();
            run_agent_seen_state(sender, agent, focused).await;
        }
    })
    .distinct_until_changed()
    .box_it()
}

fn raw_agent_for_window(window: WindowNode) -> Observable<Option<Agent>> {
    source::switch_map(window.id().box_it(), |window_id| {
        source::switch_map(agent_session_for_window(window_id), |session| {
            session
                .map(agent_session)
                .unwrap_or_else(|| source::once(None))
        })
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

fn agent_session_for_window(window_id: u64) -> Observable<Option<AgentSession>> {
    let subject = window_subject(window_id);
    source::switch_map(
        agent_session_for_window_id(window_id),
        move |session| match session {
            Some(session) => source::once(Some(session)).box_it(),
            None => agent_session_target(subject.clone())
                .map(|target| target.and_then(|target| AgentSession::from_target(&target)))
                .box_it(),
        },
    )
    .distinct_until_changed()
    .box_it()
}

fn agent_session_for_window_id(window_id: u64) -> Observable<Option<AgentSession>> {
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

fn agent_session_target(subject: RelationEndpoint) -> Observable<Option<RelationEndpoint>> {
    locus_targets(subject, WINDOW_AGENT_RELATION)
        .map(|targets| targets.into_iter().next())
        .distinct_until_changed()
        .box_it()
}

fn locus_targets(
    subject: RelationEndpoint,
    relation: &'static str,
) -> Observable<Vec<RelationEndpoint>> {
    let key = format!("{subject:?}:{relation}");
    source::shared_by_key("rsynapse.locus-targets", key, move || {
        let subject = subject.clone();
        source::from_task(move |sender| {
            let subject = subject.clone();
            async move {
                let Err(error) = run_locus_targets(sender, subject.clone(), relation).await else {
                    return;
                };
                eprintln!(
                    "[agent-source] failed to watch locus targets for {subject:?}/{relation}: {error}"
                );
            }
        })
        .distinct_until_changed()
        .box_it()
    })
}

async fn run_locus_targets(
    sender: async_channel::Sender<Result<Vec<RelationEndpoint>, String>>,
    subject: RelationEndpoint,
    relation: &'static str,
) -> Result<(), String> {
    let connection = Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    let proxy = locus_proxy(&connection)
        .await
        .map_err(|error| format!("connect locus proxy: {error}"))?;

    send_targets(&sender, &proxy, &subject, relation).await?;

    let mut added = Box::pin(
        proxy
            .receive_signal("RelationAdded")
            .await
            .map_err(to_string)?,
    );
    let mut updated = Box::pin(
        proxy
            .receive_signal("RelationUpdated")
            .await
            .map_err(to_string)?,
    );
    let mut removed = Box::pin(
        proxy
            .receive_signal("RelationRemoved")
            .await
            .map_err(to_string)?,
    );
    let mut cleared = Box::pin(
        proxy
            .receive_signal("RelationCleared")
            .await
            .map_err(to_string)?,
    );

    loop {
        tokio::select! {
            message = added.next() => {
                let Some(message) = message else { return Ok(()); };
                if relation_record_matches(&message, &subject, relation)? {
                    send_targets(&sender, &proxy, &subject, relation).await?;
                }
            }
            message = updated.next() => {
                let Some(message) = message else { return Ok(()); };
                if relation_record_matches(&message, &subject, relation)? {
                    send_targets(&sender, &proxy, &subject, relation).await?;
                }
            }
            message = removed.next() => {
                let Some(message) = message else { return Ok(()); };
                if relation_record_matches(&message, &subject, relation)? {
                    send_targets(&sender, &proxy, &subject, relation).await?;
                }
            }
            message = cleared.next() => {
                let Some(message) = message else { return Ok(()); };
                if clear_matches(&message, &subject, relation)? {
                    send_targets(&sender, &proxy, &subject, relation).await?;
                }
            }
        }
    }
}

async fn send_targets(
    sender: &async_channel::Sender<Result<Vec<RelationEndpoint>, String>>,
    proxy: &Proxy<'_>,
    subject: &RelationEndpoint,
    relation: &str,
) -> Result<(), String> {
    let targets = match proxy
        .call::<_, _, Vec<RelationEndpoint>>("Targets", &(subject, relation))
        .await
    {
        Ok(targets) => targets,
        Err(error) if is_locus_unavailable(&error) => Vec::new(),
        Err(error) => return Err(format!("read locus targets: {error}")),
    };
    sender
        .send(Ok(targets))
        .await
        .map_err(|_| "locus targets subscriber dropped".to_string())
}

async fn locus_proxy(connection: &Connection) -> zbus::Result<Proxy<'_>> {
    Proxy::new(
        connection,
        locus::BUS_NAME,
        locus::OBJECT_PATH,
        locus::RELATIONS_INTERFACE,
    )
    .await
}

fn relation_record_matches(
    message: &zbus::Message,
    subject: &RelationEndpoint,
    relation: &str,
) -> Result<bool, String> {
    let record = message
        .body()
        .deserialize::<RelationRecord>()
        .map_err(|error| format!("decode locus relation signal: {error}"))?;
    Ok(record.subject == *subject && record.relation == relation)
}

fn clear_matches(
    message: &zbus::Message,
    subject: &RelationEndpoint,
    relation: &str,
) -> Result<bool, String> {
    let (cleared_subject, cleared_relation, _count) = message
        .body()
        .deserialize::<(RelationEndpoint, String, u32)>()
        .map_err(|error| format!("decode locus clear signal: {error}"))?;
    Ok(cleared_subject == *subject && cleared_relation == relation)
}

fn agent_session(session: AgentSession) -> Observable<Option<Agent>> {
    combine_latest!(
        session.agent_name(),
        session.agent_nickname(),
        session.agent_role(),
        session.state(),
        session.requires_attention(),
        session.context_pct()
            => |(agent_name, nickname, role, state, attention, context_pct)| {
                Some(Agent {
                    icon: agent_icon(&agent_name, &nickname, &role),
                    attention,
                    state: session_state(&state),
                    context_pct: context_pct_percent(context_pct),
                    unseen: false,
                })
            },
    )
    .distinct_until_changed()
    .box_it()
}

impl AgentSession {
    fn from_target(target: &RelationEndpoint) -> Option<Self> {
        let RelationEndpoint::StableKey { kind, id } = target else {
            return None;
        };
        if kind != keys::AGENT_SESSION_ID {
            return None;
        }
        let key = id;
        let path = format!("{AGENT_SESSION_PREFIX}{key}");
        let path = OwnedObjectPath::try_from(path).ok()?;
        Some(Self { path })
    }

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

    fn context_pct(&self) -> Observable<f64> {
        required(self.property("ContextPct"), 0.0)
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

fn window_subject(id: u64) -> RelationEndpoint {
    RelationEndpoint::stable_key(keys::NIRI_WINDOW_ID, id.to_string())
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

fn context_pct_percent(value: f64) -> u32 {
    if !value.is_finite() {
        return 0;
    }
    value.round().clamp(0.0, 100.0) as u32
}

fn to_string(error: zbus::Error) -> String {
    error.to_string()
}

fn is_locus_unavailable(error: &zbus::Error) -> bool {
    match error {
        zbus::Error::MethodError(name, _, _) => {
            name.as_str() == "org.freedesktop.DBus.Error.ServiceUnknown"
        }
        zbus::Error::FDO(error) => {
            matches!(error.as_ref(), zbus::fdo::Error::ServiceUnknown(_))
        }
        _ => false,
    }
}
