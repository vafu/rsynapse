use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use async_channel::Sender;
use futures_util::StreamExt;
use locus::{RelationEndpoint, keys};
use zbus::{
    Connection, MatchRule, Message, MessageStream, Proxy,
    message::Type,
    names::BusName,
    names::OwnedInterfaceName,
    zvariant::{OwnedObjectPath, OwnedValue},
};

use crate::rx::Observable as _;
use crate::{Observable, from_task, shared_by_key, watch_relation_subject};

const BZBUS_BUS: &str = "com.snap.BzBus";
const BZBUS_OBJECT_PATH: &str = "/com/snap/BzBus";
const BZBUS_INVOCATIONS_PATH: &str = "/com/snap/BzBus/invocations";
const DBUS_BUS: &str = "org.freedesktop.DBus";
const DBUS_BUS_PATH: &str = "/org/freedesktop/DBus";
const DBUS_BUS_INTERFACE: &str = "org.freedesktop.DBus";
const DBUS_OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";
const DBUS_PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const PROPERTIES_CHANGED: &str = "PropertiesChanged";
const INVOCATION_INTERFACE: &str = "com.snap.BzBus.Invocation1";
const INTERFACES_ADDED: &str = "InterfacesAdded";
const INTERFACES_REMOVED: &str = "InterfacesRemoved";
const WINDOW_BUILD_INVOCATION_RELATION: &str = "org.rsynapse.window.build-invocation";
const ACTIVE_STALE_MS: i64 = 2 * 60 * 60 * 1000;

const BZBUS_PROPERTIES: &[&str] = &[
    "Id",
    "BuildId",
    "Component",
    "Source",
    "LastObservedSequenceNumber",
    "Status",
    "Outcome",
    "CommandName",
    "ProgressCompleted",
    "ProgressTotal",
    "ActionsCompleted",
    "TotalActions",
    "ActionsFailed",
    "RunningActions",
    "StartedAtUnixMs",
    "EndedAtUnixMs",
];

type PropertyMap = HashMap<String, OwnedValue>;
type InterfaceMap = HashMap<OwnedInterfaceName, PropertyMap>;
type RawManagedObjects = HashMap<OwnedObjectPath, InterfaceMap>;
type InvocationMap = HashMap<OwnedObjectPath, Invocation>;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Invocation {
    pub id: String,
    pub build_id: String,
    pub component: String,
    pub source: String,
    pub last_sequence: i64,
    pub status: String,
    pub outcome: String,
    pub command_name: String,
    pub started_at_unix_ms: i64,
    pub ended_at_unix_ms: i64,
    pub progress_completed: u32,
    pub progress_total: u32,
    pub actions_completed: u32,
    pub total_actions: u64,
    pub actions_failed: u32,
    pub running_actions: u32,
}

impl Invocation {
    pub fn is_active(&self) -> bool {
        !self.is_ended()
            && !self.is_stale()
            && !matches!(
                normalized(self.status.as_str()).as_str(),
                "finished" | "failed"
            )
    }

    pub fn is_failed(&self) -> bool {
        matches!(
            normalized(self.status.as_str()).as_str(),
            "failed" | "failure" | "error"
        ) || matches!(
            normalized(self.outcome.as_str()).as_str(),
            "failed" | "failure" | "error"
        )
    }

    pub fn is_finished(&self) -> bool {
        matches!(
            normalized(self.status.as_str()).as_str(),
            "finished" | "success"
        ) || matches!(
            normalized(self.outcome.as_str()).as_str(),
            "finished" | "success"
        )
    }

    pub fn is_ended(&self) -> bool {
        self.ended_at_unix_ms > 0 || self.is_failed() || self.is_finished()
    }

    pub fn is_stale(&self) -> bool {
        let last_observed = self.observed_time();
        last_observed > 0 && now_unix_ms() - last_observed > ACTIVE_STALE_MS
    }

    pub fn observed_time(&self) -> i64 {
        self.ended_at_unix_ms.max(self.started_at_unix_ms)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BzBusSnapshot {
    pub active: bool,
    pub invocations: Vec<Invocation>,
}

impl Default for BzBusSnapshot {
    fn default() -> Self {
        Self {
            active: false,
            invocations: Vec::new(),
        }
    }
}

pub fn snapshots() -> Observable<BzBusSnapshot> {
    shared_by_key("rsynapse.bzbus-snapshots", BZBUS_OBJECT_PATH, || {
        from_task(|sender| async move {
            run_bzbus_source(sender).await;
        })
        .distinct_until_changed()
        .box_it()
    })
}

pub fn ongoing_ids() -> Observable<Vec<String>> {
    snapshots()
        .map(|snapshot| ongoing_invocation_ids(&snapshot))
        .distinct_until_changed()
        .box_it()
}

pub fn invocation(id: String) -> Observable<Option<Invocation>> {
    snapshots()
        .map(move |snapshot| {
            snapshot
                .invocations
                .into_iter()
                .find(|invocation| invocation.id == id)
        })
        .distinct_until_changed()
        .box_it()
}

pub fn invocation_ids_for_window(window_id: String) -> Observable<Vec<String>> {
    let subject = RelationEndpoint::stable_key(keys::WINDOW_ID, window_id);
    let relation = WINDOW_BUILD_INVOCATION_RELATION;
    watch_relation_subject(subject, relation)
        .map(|records| {
            records
                .into_iter()
                .filter_map(|record| invocation_id_from_target(record.target))
                .collect()
        })
        .distinct_until_changed()
        .box_it()
}

fn ongoing_invocation_ids(snapshot: &BzBusSnapshot) -> Vec<String> {
    if !snapshot.active {
        return Vec::new();
    }

    let mut invocations = snapshot
        .invocations
        .iter()
        .filter(|invocation| invocation.is_active())
        .collect::<Vec<_>>();
    invocations.sort_by(|left, right| compare_invocations(left, right));
    invocations
        .into_iter()
        .map(|invocation| invocation.id.clone())
        .collect()
}

fn compare_invocations(left: &Invocation, right: &Invocation) -> std::cmp::Ordering {
    right
        .is_active()
        .cmp(&left.is_active())
        .then_with(|| right.observed_time().cmp(&left.observed_time()))
        .then_with(|| right.last_sequence.cmp(&left.last_sequence))
        .then_with(|| right.id.cmp(&left.id))
}

fn invocation_id_from_target(target: RelationEndpoint) -> Option<String> {
    let RelationEndpoint::StableKey { kind, id } = target else {
        return None;
    };
    (kind == keys::BAZEL_INVOCATION_ID).then_some(id)
}

async fn run_bzbus_source(sender: Sender<Result<BzBusSnapshot, String>>) {
    let connection = match Connection::session().await {
        Ok(connection) => connection,
        Err(error) => {
            let _ = sender
                .send(Err(format_dbus_error("connect session bus", error)))
                .await;
            return;
        }
    };

    let mut owner_changes = match bzbus_owner_changed_stream(&connection).await {
        Ok(stream) => stream,
        Err(error) => {
            let _ = sender.send(Err(error)).await;
            return;
        }
    };
    let mut latest = None;

    loop {
        if !name_has_owner(&connection).await.unwrap_or(false) {
            if !emit_snapshot(&sender, &mut latest, false, Vec::new()).await {
                return;
            }
            if wait_for_bzbus_owner(&mut owner_changes).await.is_err() {
                return;
            }
        }

        match watch_bzbus_until_owner_changes(&connection, &sender, &mut latest, &mut owner_changes)
            .await
        {
            Ok(WatchExit::Restart) => continue,
            Ok(WatchExit::Closed) => return,
            Err(error) => {
                eprintln!("[bzbus] {error}");
                if !emit_snapshot(&sender, &mut latest, false, Vec::new()).await {
                    return;
                }
                if wait_for_bzbus_owner(&mut owner_changes).await.is_err() {
                    return;
                }
            }
        }
    }
}

async fn watch_bzbus_until_owner_changes(
    connection: &Connection,
    sender: &Sender<Result<BzBusSnapshot, String>>,
    latest: &mut Option<BzBusSnapshot>,
    owner_changes: &mut MessageStream,
) -> Result<WatchExit, String> {
    let manager = object_manager_proxy(connection).await?;
    let mut object_signals = manager
        .receive_all_signals()
        .await
        .map_err(|error| format_dbus_error("subscribe BzBus ObjectManager signals", error))?;
    let mut property_signals = invocation_properties_changed_stream(connection).await?;
    let mut invocations = read_invocations(&manager).await?;

    if !emit_snapshot(
        sender,
        latest,
        true,
        invocations.values().cloned().collect(),
    )
    .await
    {
        return Ok(WatchExit::Closed);
    }

    loop {
        tokio::select! {
            owner_message = owner_changes.next() => {
                let Some(owner_message) = owner_message else {
                    return Ok(WatchExit::Closed);
                };
                match decode_bzbus_owner_changed(owner_message) {
                    Ok(Some(_)) => {
                        if !emit_snapshot(sender, latest, false, Vec::new()).await {
                            return Ok(WatchExit::Closed);
                        }
                        return Ok(WatchExit::Restart);
                    }
                    Ok(None) => continue,
                    Err(error) => return Err(error),
                }
            }
            object_message = object_signals.next() => {
                let Some(object_message) = object_message else {
                    return Ok(WatchExit::Restart);
                };
                match apply_object_manager_signal(&mut invocations, object_message) {
                    SourceChange::Changed => {
                        if !emit_snapshot(sender, latest, true, invocations.values().cloned().collect()).await {
                            return Ok(WatchExit::Closed);
                        }
                    }
                    SourceChange::Ignore => {}
                    SourceChange::Error(error) => return Err(error),
                }
            }
            property_message = property_signals.next() => {
                let Some(property_message) = property_message else {
                    return Ok(WatchExit::Restart);
                };
                let property_message = property_message
                    .map_err(|error| format_dbus_error("receive BzBus PropertiesChanged", error))?;
                match apply_properties_changed(&mut invocations, property_message) {
                    SourceChange::Changed => {
                        if !emit_snapshot(sender, latest, true, invocations.values().cloned().collect()).await {
                            return Ok(WatchExit::Closed);
                        }
                    }
                    SourceChange::Ignore => {}
                    SourceChange::Error(error) => return Err(error),
                }
            }
        }
    }
}

async fn emit_snapshot(
    sender: &Sender<Result<BzBusSnapshot, String>>,
    latest: &mut Option<BzBusSnapshot>,
    active: bool,
    invocations: Vec<Invocation>,
) -> bool {
    let snapshot = BzBusSnapshot {
        active,
        invocations,
    };
    if latest.as_ref() == Some(&snapshot) {
        return true;
    }

    *latest = Some(snapshot.clone());
    sender.send(Ok(snapshot)).await.is_ok()
}

fn invocations_from_managed_objects(objects: RawManagedObjects) -> InvocationMap {
    objects
        .into_iter()
        .filter_map(|(path, interfaces)| {
            interfaces
                .into_iter()
                .find(|(interface, _)| interface.as_str() == INVOCATION_INTERFACE)
                .map(|(_, properties)| {
                    let invocation = invocation_from_properties(&path, &properties);
                    (path, invocation)
                })
        })
        .collect()
}

fn apply_object_manager_signal(invocations: &mut InvocationMap, message: Message) -> SourceChange {
    let Some(member) = message.header().member().map(|member| member.to_string()) else {
        return SourceChange::Ignore;
    };

    match member.as_str() {
        INTERFACES_ADDED => {
            let body = message
                .body()
                .deserialize::<(OwnedObjectPath, InterfaceMap)>();
            match body {
                Ok((path, interfaces)) => {
                    if let Some((_, properties)) = interfaces
                        .into_iter()
                        .find(|(interface, _)| interface.as_str() == INVOCATION_INTERFACE)
                    {
                        let invocation = invocation_from_properties(&path, &properties);
                        invocations.insert(path, invocation);
                        SourceChange::Changed
                    } else {
                        SourceChange::Ignore
                    }
                }
                Err(error) => {
                    SourceChange::Error(format_dbus_error("decode BzBus InterfacesAdded", error))
                }
            }
        }
        INTERFACES_REMOVED => {
            let body = message
                .body()
                .deserialize::<(OwnedObjectPath, Vec<OwnedInterfaceName>)>();
            match body {
                Ok((path, interfaces)) => {
                    if interfaces
                        .iter()
                        .any(|interface| interface.as_str() == INVOCATION_INTERFACE)
                        && invocations.remove(&path).is_some()
                    {
                        SourceChange::Changed
                    } else {
                        SourceChange::Ignore
                    }
                }
                Err(error) => {
                    SourceChange::Error(format_dbus_error("decode BzBus InterfacesRemoved", error))
                }
            }
        }
        _ => SourceChange::Ignore,
    }
}

fn apply_properties_changed(invocations: &mut InvocationMap, message: Message) -> SourceChange {
    let Some(path) = message
        .header()
        .path()
        .map(|path| OwnedObjectPath::from(path.to_owned()))
    else {
        return SourceChange::Error(
            "BzBus PropertiesChanged signal missing object path".to_owned(),
        );
    };
    let body = message
        .body()
        .deserialize::<(OwnedInterfaceName, PropertyMap, Vec<String>)>();
    let (interface, changed, invalidated) = match body {
        Ok(body) => body,
        Err(error) => {
            return SourceChange::Error(format_dbus_error("decode BzBus PropertiesChanged", error));
        }
    };

    if interface.as_str() != INVOCATION_INTERFACE {
        return SourceChange::Ignore;
    }

    if !changed.keys().any(|name| is_bzbus_property(name))
        && !invalidated.iter().any(|name| is_bzbus_property(name))
    {
        return SourceChange::Ignore;
    }

    let invocation = invocations
        .entry(path.clone())
        .or_insert_with(|| invocation_from_properties(&path, &HashMap::new()));
    apply_invocation_properties(invocation, &changed);
    for property in invalidated {
        clear_invocation_property(invocation, property.as_str());
    }
    SourceChange::Changed
}

fn invocation_from_properties(path: &OwnedObjectPath, properties: &PropertyMap) -> Invocation {
    let mut invocation = Invocation {
        id: invocation_id_from_path(path),
        ..Invocation::default()
    };
    apply_invocation_properties(&mut invocation, properties);
    if invocation.id.trim().is_empty() {
        invocation.id = invocation_id_from_path(path);
    }
    invocation
}

fn apply_invocation_properties(invocation: &mut Invocation, properties: &PropertyMap) {
    for (name, value) in properties {
        match name.as_str() {
            "Id" => set_string(&mut invocation.id, value),
            "BuildId" => set_string(&mut invocation.build_id, value),
            "Component" => set_string(&mut invocation.component, value),
            "Source" => set_string(&mut invocation.source, value),
            "LastObservedSequenceNumber" => set_i64(&mut invocation.last_sequence, value),
            "Status" => set_string(&mut invocation.status, value),
            "Outcome" => set_string(&mut invocation.outcome, value),
            "CommandName" => set_string(&mut invocation.command_name, value),
            "ProgressCompleted" => set_u32(&mut invocation.progress_completed, value),
            "ProgressTotal" => set_u32(&mut invocation.progress_total, value),
            "ActionsCompleted" => set_u32(&mut invocation.actions_completed, value),
            "TotalActions" => set_u64(&mut invocation.total_actions, value),
            "ActionsFailed" => set_u32(&mut invocation.actions_failed, value),
            "RunningActions" => set_u32(&mut invocation.running_actions, value),
            "StartedAtUnixMs" => set_i64(&mut invocation.started_at_unix_ms, value),
            "EndedAtUnixMs" => set_i64(&mut invocation.ended_at_unix_ms, value),
            _ => {}
        }
    }
}

fn clear_invocation_property(invocation: &mut Invocation, property: &str) {
    match property {
        "Id" => invocation.id.clear(),
        "BuildId" => invocation.build_id.clear(),
        "Component" => invocation.component.clear(),
        "Source" => invocation.source.clear(),
        "LastObservedSequenceNumber" => invocation.last_sequence = 0,
        "Status" => invocation.status.clear(),
        "Outcome" => invocation.outcome.clear(),
        "CommandName" => invocation.command_name.clear(),
        "ProgressCompleted" => invocation.progress_completed = 0,
        "ProgressTotal" => invocation.progress_total = 0,
        "ActionsCompleted" => invocation.actions_completed = 0,
        "TotalActions" => invocation.total_actions = 0,
        "ActionsFailed" => invocation.actions_failed = 0,
        "RunningActions" => invocation.running_actions = 0,
        "StartedAtUnixMs" => invocation.started_at_unix_ms = 0,
        "EndedAtUnixMs" => invocation.ended_at_unix_ms = 0,
        _ => {}
    }
}

fn is_bzbus_property(name: &str) -> bool {
    BZBUS_PROPERTIES.contains(&name)
}

fn set_string(target: &mut String, value: &OwnedValue) {
    if let Some(value) = string_value(value) {
        *target = value;
    }
}

fn set_i64(target: &mut i64, value: &OwnedValue) {
    if let Some(value) = i64_value(value) {
        *target = value;
    }
}

fn set_u32(target: &mut u32, value: &OwnedValue) {
    if let Some(value) = u32_value(value) {
        *target = value;
    }
}

fn set_u64(target: &mut u64, value: &OwnedValue) {
    if let Some(value) = u64_value(value) {
        *target = value;
    }
}

fn string_value(value: &OwnedValue) -> Option<String> {
    value
        .try_clone()
        .ok()
        .and_then(|value| String::try_from(value).ok())
}

fn i64_value(value: &OwnedValue) -> Option<i64> {
    i64::try_from(value)
        .ok()
        .or_else(|| i32::try_from(value).ok().map(i64::from))
        .or_else(|| u32::try_from(value).ok().map(i64::from))
        .or_else(|| {
            u64::try_from(value)
                .ok()
                .and_then(|value| i64::try_from(value).ok())
        })
}

fn u32_value(value: &OwnedValue) -> Option<u32> {
    u32::try_from(value)
        .ok()
        .or_else(|| i64_value(value).and_then(|value| u32::try_from(value).ok()))
}

fn u64_value(value: &OwnedValue) -> Option<u64> {
    u64::try_from(value)
        .ok()
        .or_else(|| u32::try_from(value).ok().map(u64::from))
        .or_else(|| {
            i64::try_from(value)
                .ok()
                .and_then(|value| u64::try_from(value).ok())
        })
}

fn invocation_id_from_path(path: &OwnedObjectPath) -> String {
    path.as_str()
        .rsplit('/')
        .next()
        .unwrap_or(path.as_str())
        .to_owned()
}

async fn object_manager_proxy(connection: &Connection) -> Result<Proxy<'_>, String> {
    Proxy::new(
        connection,
        BZBUS_BUS,
        BZBUS_OBJECT_PATH,
        DBUS_OBJECT_MANAGER,
    )
    .await
    .map_err(|error| format_dbus_error("connect BzBus ObjectManager", error))
}

async fn read_invocations(manager: &Proxy<'_>) -> Result<InvocationMap, String> {
    let objects = manager
        .call::<_, _, RawManagedObjects>("GetManagedObjects", &())
        .await
        .map_err(|error| format_dbus_error("read BzBus managed objects", error))?;

    Ok(invocations_from_managed_objects(objects))
}

async fn invocation_properties_changed_stream(
    connection: &Connection,
) -> Result<MessageStream, String> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .interface(DBUS_PROPERTIES)
        .map_err(|error| format_dbus_error("build BzBus PropertiesChanged match", error))?
        .member(PROPERTIES_CHANGED)
        .map_err(|error| format_dbus_error("build BzBus PropertiesChanged match", error))?
        .path_namespace(BZBUS_INVOCATIONS_PATH)
        .map_err(|error| format_dbus_error("build BzBus PropertiesChanged match", error))?
        .add_arg(INVOCATION_INTERFACE)
        .map_err(|error| format_dbus_error("build BzBus PropertiesChanged match", error))?
        .build()
        .to_owned();

    MessageStream::for_match_rule(rule, connection, Some(512))
        .await
        .map_err(|error| format_dbus_error("subscribe BzBus PropertiesChanged", error))
}

async fn bzbus_owner_changed_stream(connection: &Connection) -> Result<MessageStream, String> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(DBUS_BUS)
        .map_err(|error| format_dbus_error("build BzBus owner match", error))?
        .path(DBUS_BUS_PATH)
        .map_err(|error| format_dbus_error("build BzBus owner match", error))?
        .interface(DBUS_BUS_INTERFACE)
        .map_err(|error| format_dbus_error("build BzBus owner match", error))?
        .member("NameOwnerChanged")
        .map_err(|error| format_dbus_error("build BzBus owner match", error))?
        .add_arg(BZBUS_BUS)
        .map_err(|error| format_dbus_error("build BzBus owner match", error))?
        .build()
        .to_owned();

    MessageStream::for_match_rule(rule, connection, Some(16))
        .await
        .map_err(|error| format_dbus_error("subscribe BzBus owner changes", error))
}

async fn name_has_owner(connection: &Connection) -> Result<bool, String> {
    let proxy = zbus::fdo::DBusProxy::new(connection)
        .await
        .map_err(|error| format_dbus_error("connect D-Bus daemon", error))?;
    let name = BusName::try_from(BZBUS_BUS)
        .map_err(|error| format!("invalid BzBus bus name {BZBUS_BUS}: {error}"))?;
    proxy
        .name_has_owner(name)
        .await
        .map_err(|error| format_dbus_error("check BzBus owner", error))
}

async fn wait_for_bzbus_owner(owner_changes: &mut MessageStream) -> Result<(), String> {
    while let Some(message) = owner_changes.next().await {
        if decode_bzbus_owner_changed(message)?.is_some_and(|owned| owned) {
            return Ok(());
        }
    }
    Err("BzBus owner change stream closed".to_owned())
}

fn decode_bzbus_owner_changed(message: zbus::Result<Message>) -> Result<Option<bool>, String> {
    let message =
        message.map_err(|error| format_dbus_error("receive BzBus owner change", error))?;
    let (name, _old_owner, new_owner) = message
        .body()
        .deserialize::<(String, String, String)>()
        .map_err(|error| format_dbus_error("decode BzBus owner change", error))?;

    if name == BZBUS_BUS {
        Ok(Some(!new_owner.is_empty()))
    } else {
        Ok(None)
    }
}

fn format_dbus_error(context: &str, error: impl std::fmt::Display) -> String {
    format!("{context} failed: {error}")
}

fn normalized(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

enum SourceChange {
    Changed,
    Ignore,
    Error(String),
}

enum WatchExit {
    Restart,
    Closed,
}

#[cfg(test)]
mod tests {
    use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

    use super::*;

    #[test]
    fn invocation_from_properties_decodes_bzbus_dbus_payload() {
        let path = OwnedObjectPath::try_from(
            "/com/snap/BzBus/invocations/invocation_4c4bf6a1_4e5b_4fd8_b6a0_c03d50ed1b33",
        )
        .unwrap();
        let properties = properties(&[
            ("Id", value("invocation-live")),
            ("BuildId", value("build-1")),
            ("Component", value("TOOL")),
            ("Source", value("bes")),
            ("LastObservedSequenceNumber", value(99i64)),
            ("Status", value("running")),
            ("Outcome", value("unknown")),
            ("CommandName", value("run")),
            ("ProgressCompleted", value(72_427u32)),
            ("ProgressTotal", value(72_435u32)),
            ("ActionsCompleted", value(8u32)),
            ("TotalActions", value(12i64)),
            ("ActionsFailed", value(1u32)),
            ("RunningActions", value(3u32)),
            ("StartedAtUnixMs", value(1_783_556_374_071i64)),
            ("EndedAtUnixMs", value(0i64)),
        ]);

        let invocation = invocation_from_properties(&path, &properties);

        assert_eq!(invocation.id, "invocation-live");
        assert_eq!(invocation.build_id, "build-1");
        assert_eq!(invocation.component, "TOOL");
        assert_eq!(invocation.source, "bes");
        assert_eq!(invocation.last_sequence, 99);
        assert_eq!(invocation.status, "running");
        assert_eq!(invocation.outcome, "unknown");
        assert_eq!(invocation.command_name, "run");
        assert_eq!(invocation.progress_completed, 72_427);
        assert_eq!(invocation.progress_total, 72_435);
        assert_eq!(invocation.actions_completed, 8);
        assert_eq!(invocation.total_actions, 12);
        assert_eq!(invocation.actions_failed, 1);
        assert_eq!(invocation.running_actions, 3);
        assert_eq!(invocation.started_at_unix_ms, 1_783_556_374_071);
        assert_eq!(invocation.ended_at_unix_ms, 0);
    }

    #[test]
    fn invocation_uses_path_id_when_id_property_is_missing() {
        let path = OwnedObjectPath::try_from(
            "/com/snap/BzBus/invocations/invocation_fd898886_c248_437c_8a1a_5d374dc68888",
        )
        .unwrap();

        let invocation = invocation_from_properties(&path, &HashMap::new());

        assert_eq!(
            invocation.id,
            "invocation_fd898886_c248_437c_8a1a_5d374dc68888"
        );
    }

    #[test]
    fn apply_invocation_properties_updates_existing_invocation() {
        let mut invocation = Invocation {
            id: "existing".to_owned(),
            status: "queued".to_owned(),
            progress_completed: 1,
            progress_total: 4,
            total_actions: 0,
            ..Invocation::default()
        };
        let changed = properties(&[
            ("Status", value("running")),
            ("ProgressCompleted", value(3u32)),
            ("TotalActions", value(9i64)),
        ]);

        apply_invocation_properties(&mut invocation, &changed);

        assert_eq!(invocation.id, "existing");
        assert_eq!(invocation.status, "running");
        assert_eq!(invocation.progress_completed, 3);
        assert_eq!(invocation.progress_total, 4);
        assert_eq!(invocation.total_actions, 9);
    }

    #[test]
    fn numeric_values_accept_bzbus_integer_shapes() {
        assert_eq!(
            i64_value(&value(1_782_436_305_258i64)),
            Some(1_782_436_305_258)
        );
        assert_eq!(i64_value(&value(17u32)), Some(17));
        assert_eq!(u32_value(&value(17u32)), Some(17));
        assert_eq!(u32_value(&value(17i64)), Some(17));
        assert_eq!(u64_value(&value(13261i64)), Some(13261));
    }

    #[test]
    fn ongoing_ids_ignore_progress_only_changes() {
        let before = snapshot("invocation-1", 2, 100);
        let after = snapshot("invocation-1", 97, 100);

        assert_eq!(
            ongoing_invocation_ids(&before),
            ongoing_invocation_ids(&after)
        );
        assert_eq!(ongoing_invocation_ids(&before), vec!["invocation-1"]);
    }

    #[test]
    fn ongoing_ids_change_for_membership_changes() {
        let one = snapshot("invocation-1", 2, 100);
        let mut two = one.clone();
        two.invocations.push(running_invocation("invocation-2"));

        assert_eq!(ongoing_invocation_ids(&one), vec!["invocation-1"]);
        assert_eq!(
            ongoing_invocation_ids(&two),
            vec!["invocation-2", "invocation-1"]
        );
    }

    fn snapshot(id: &str, progress_completed: u32, progress_total: u32) -> BzBusSnapshot {
        BzBusSnapshot {
            active: true,
            invocations: vec![Invocation {
                id: id.to_owned(),
                progress_completed,
                progress_total,
                ..running_invocation(id)
            }],
        }
    }

    fn running_invocation(id: &str) -> Invocation {
        Invocation {
            id: id.to_owned(),
            status: "running".to_owned(),
            started_at_unix_ms: now_unix_ms() - 1_000,
            progress_total: 100,
            ..Invocation::default()
        }
    }

    fn properties(entries: &[(&str, OwnedValue)]) -> PropertyMap {
        entries
            .iter()
            .map(|(name, value)| (name.to_string(), value.try_clone().unwrap()))
            .collect()
    }

    fn value<T>(value: T) -> OwnedValue
    where
        Value<'static>: From<T>,
    {
        OwnedValue::try_from(Value::from(value)).unwrap()
    }
}
