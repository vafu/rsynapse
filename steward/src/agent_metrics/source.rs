use super::{Session, Snapshot, UsageEvent};
use crate::relations::{self, LocusClient};
use futures_util::StreamExt;
use locus::RelationRecord;
use shell_rx_macros::combine_latest;
use shell_source::{
    Observable,
    dbus::{self, Bus, ObjectDescriptor, ObjectManagerDescriptor, ObjectModel, PropertyDescriptor},
    rx::Observable as _,
};
use std::collections::HashMap;
use zbus::zvariant::OwnedObjectPath;

const AGENT_BUS: &str = "io.github.AgentDBus";
const AGENT_ROOT: &str = "/io/github/AgentDBus";
const AGENT_INTERFACE: &str = "io.github.AgentDBus1.Session";

#[derive(Clone, Debug, PartialEq)]
struct AgentObject(OwnedObjectPath);
impl ObjectModel for AgentObject {
    const INTERFACE: &'static str = AGENT_INTERFACE;
    fn at(path: OwnedObjectPath) -> Self {
        Self(path)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Window(OwnedObjectPath);
impl ObjectModel for Window {
    const INTERFACE: &'static str = niri_dbus::WINDOW_INTERFACE;
    fn at(path: OwnedObjectPath) -> Self {
        Self(path)
    }
}

pub(crate) fn snapshots(locus: &LocusClient) -> Observable<Snapshot> {
    let sessions = dbus::models::<AgentObject>(
        ObjectManagerDescriptor::parse(Bus::Session, AGENT_BUS, AGENT_ROOT)
            .expect("AgentDBus descriptor"),
    );
    let sessions = shell_source::switch_map_list(sessions, session_values);
    let windows = dbus::models::<Window>(
        ObjectManagerDescriptor::parse(Bus::Session, niri_dbus::BUS_NAME, niri_dbus::ROOT_PATH)
            .expect("niri descriptor"),
    );
    let windows = shell_source::switch_map_list(windows, window_values);
    combine_latest!(
        sessions,
        windows,
        relations::records(locus.clone(), relations::WORKSPACE_PROJECT),
        relations::records(locus.clone(), relations::WORKSPACE_NAME),
        relations::records(locus.clone(), relations::WINDOW_AGENT_SESSION)
        => |(sessions, windows, projects, names, links)| {
            let windows = windows.into_iter().filter_map(|(window, workspace)| workspace.map(|ws| (window, ws))).collect::<HashMap<_, _>>();
            let sessions = super::attribution::attribute(sessions, &windows, &projects, &names);
            Snapshot { sessions, active_links: active_links(&links) }
        },
    ).distinct_until_changed().box_it()
}

fn session_values(object: AgentObject) -> Observable<Session> {
    let descriptor =
        ObjectDescriptor::parse(Bus::Session, AGENT_BUS, object.0.as_str(), AGENT_INTERFACE)
            .expect("agent descriptor");
    let property = |name| PropertyDescriptor::new(descriptor.clone(), name);
    combine_latest!(
        dbus::property_or(property("SessionId"), String::new()),
        dbus::property_or(property("AgentName"), String::new()),
        dbus::property_or(property("State"), String::new()),
        dbus::property_or(property("Cwd"), String::new()),
        dbus::property_or(property("WindowId"), String::new()),
        dbus::property_or(property("ParentSessionId"), String::new()),
        dbus::property_or(property("IsSubagent"), false),
        dbus::property_or(property("ModelName"), "unknown".to_owned()),
        dbus::property_or(property("ReasoningEffort"), "unknown".to_owned())
        => move |(session_id, agent, state, cwd, window, parent, subagent, model, effort)| Session {
            key: object.0.as_str().to_owned(), session_id,
            agent: if agent.trim().is_empty() { object.0.as_str().split('/').nth(5).unwrap_or("unknown").to_owned() } else { crate::focus::canonical_app_id(agent.trim()) },
            state: if state.trim().is_empty() { "unknown".to_owned() } else { state },
            cwd, window: window.parse().ok(), parent, subagent, model, effort,
            ..Session::default()
        },
    ).distinct_until_changed().box_it()
}

/// One namespace subscription avoids losing reports when roster subscriptions churn.
/// The backend task is exposed as a shell-source observable, like other inputs.
pub(crate) fn usage_events() -> Observable<UsageEvent> {
    type Report = (
        u64,
        u64,
        String,
        String,
        String,
        String,
        HashMap<String, u64>,
        HashMap<String, u64>,
    );
    shell_source::from_task(|sender| async move {
        let run = async {
            let conn = zbus::Connection::session()
                .await
                .map_err(|e| e.to_string())?;
            let rule = zbus::MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .sender(AGENT_BUS)
                .map_err(|e| e.to_string())?
                .interface(AGENT_INTERFACE)
                .map_err(|e| e.to_string())?
                .member("TokenUsageReported")
                .map_err(|e| e.to_string())?
                .path_namespace(AGENT_ROOT)
                .map_err(|e| e.to_string())?
                .build();
            let mut stream = zbus::MessageStream::for_match_rule(rule, &conn, Some(256))
                .await
                .map_err(|e| e.to_string())?;
            while let Some(message) = stream.next().await {
                let message = message.map_err(|e| e.to_string())?;
                let header = message.header();
                let Some(path) = header.path() else {
                    continue;
                };
                let (report,): (Report,) =
                    message.body().deserialize().map_err(|e| e.to_string())?;
                let (epoch, revision, _timestamp, _turn, model, effort, delta, _totals) = report;
                let event = UsageEvent {
                    key: path.as_str().to_owned(),
                    owner: header
                        .sender()
                        .map(|s| s.as_str().to_owned())
                        .unwrap_or_default(),
                    epoch,
                    revision,
                    model,
                    effort,
                    delta,
                };
                if sender.send(Ok(event)).await.is_err() {
                    return Ok::<(), String>(());
                }
            }
            Err("AgentDBus usage signal stream ended".to_owned())
        }
        .await;
        if let Err(error) = run {
            let _ = sender.send(Err(error)).await;
        }
    })
}

fn window_values(window: Window) -> Observable<(u64, Option<u64>)> {
    let id = window
        .0
        .as_str()
        .rsplit("window_")
        .next()
        .and_then(|id| id.parse().ok())
        .unwrap_or(0);
    let descriptor = ObjectDescriptor::parse(
        Bus::Session,
        niri_dbus::BUS_NAME,
        window.0.as_str(),
        niri_dbus::WINDOW_INTERFACE,
    )
    .expect("window descriptor");
    dbus::optional_array_property::<OwnedObjectPath>(PropertyDescriptor::new(
        descriptor,
        "Workspace",
    ))
    .map(move |path| {
        (
            id,
            path.and_then(|p| p.as_str().rsplit("workspace_").next()?.parse().ok()),
        )
    })
    .distinct_until_changed()
    .box_it()
}

fn active_links(records: &[RelationRecord]) -> HashMap<u64, String> {
    records
        .iter()
        .filter_map(|r| {
            let locus::RelationEndpoint::StableKey { kind, id } = &r.subject else {
                return None;
            };
            if kind != locus::keys::NIRI_WINDOW_ID {
                return None;
            }
            let path = r
                .metadata
                .get("session_path")
                .cloned()
                .or_else(|| match &r.target {
                    locus::RelationEndpoint::DBusObject {
                        service,
                        interface,
                        path,
                        ..
                    } if service == AGENT_BUS && interface == AGENT_INTERFACE => Some(path.clone()),
                    _ => None,
                })?;
            Some((id.parse().ok()?, path))
        })
        .collect()
}
