use futures_util::StreamExt;
use locus::{RelationEndpoint, RelationRecord, keys};
use shell_core::source::{
    self, Observable,
    dbus::{self, Bus, ObjectDescriptor, PropertyDescriptor},
    rx::Observable as _,
};
use shell_rx_macros::combine_latest;
use zbus::{Connection, Proxy, zvariant::OwnedObjectPath};

use super::super::{Agent, State};
use crate::widgets::bar::WindowNode;

const AGENT_DBUS_BUS: &str = "io.github.AgentDBus";
const AGENT_SESSION_INTERFACE: &str = "io.github.AgentDBus1.Session";
const AGENT_SESSION_PREFIX: &str = "/io/github/AgentDBus/sessions/";
const WINDOW_AGENT_RELATION: &str = "org.rsynapse.window.agent-session";

#[derive(Clone, Debug, Eq, PartialEq)]
struct AgentSession {
    path: OwnedObjectPath,
}

pub(super) fn agent_for_window(window: WindowNode) -> Observable<Option<Agent>> {
    source::switch_map(window.id().map(window_subject).box_it(), |subject| {
        source::switch_map(agent_session_target(subject), |target| {
            target
                .and_then(|target| AgentSession::from_target(&target))
                .map(agent_session)
                .unwrap_or_else(|| source::once(None))
        })
    })
    .distinct_until_changed()
    .box_it()
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

fn session_state(state: &str) -> State {
    match state {
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
