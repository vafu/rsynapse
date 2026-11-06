use async_channel::Sender;
use futures_util::StreamExt;
use locus::{RelationEndpoint, keys};
use shell_core::source::{self, Observable, rx::Observable as _};
use shell_rx_macros::combine_latest;
use zbus::{Connection, MatchRule, Message, MessageStream, Proxy, message::Type, names::BusName};

use super::view::{self, BzBusView, Invocation};
use crate::widgets::bar::WindowNode;
use protocol::{
    INVOCATION_INTERFACE, InvocationMap, RawManagedObjects, SourceChange,
    apply_object_manager_signal, apply_properties_changed, invocations_from_managed_objects,
};

mod protocol;

#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

#[cfg(test)]
pub(super) fn invocation_from_properties(
    path: &OwnedObjectPath,
    properties: &HashMap<String, OwnedValue>,
) -> Invocation {
    protocol::invocation_from_properties(path, properties)
}

#[cfg(test)]
pub(super) fn apply_invocation_properties(
    invocation: &mut Invocation,
    properties: &HashMap<String, OwnedValue>,
) {
    protocol::apply_invocation_properties(invocation, properties);
}

#[cfg(test)]
pub(super) fn i64_value(value: &OwnedValue) -> Option<i64> {
    protocol::i64_value(value)
}

#[cfg(test)]
pub(super) fn u32_value(value: &OwnedValue) -> Option<u32> {
    protocol::u32_value(value)
}

#[cfg(test)]
pub(super) fn u64_value(value: &OwnedValue) -> Option<u64> {
    protocol::u64_value(value)
}

const BZBUS_BUS: &str = "com.snap.BzBus";
const BZBUS_OBJECT_PATH: &str = "/com/snap/BzBus";
const BZBUS_INVOCATIONS_PATH: &str = "/com/snap/BzBus/invocations";
const DBUS_BUS: &str = "org.freedesktop.DBus";
const DBUS_BUS_PATH: &str = "/org/freedesktop/DBus";
const DBUS_BUS_INTERFACE: &str = "org.freedesktop.DBus";
const DBUS_OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";
const DBUS_PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const PROPERTIES_CHANGED: &str = "PropertiesChanged";
const WINDOW_BUILD_INVOCATION_RELATION: &str = "org.rsynapse.window.build-invocation";

#[derive(Clone, Debug, Eq, PartialEq)]
struct BzBusSnapshot {
    active: bool,
    invocations: Vec<Invocation>,
}

pub(in crate::widgets::bar) fn bzbus_for_window(
    window: WindowNode,
) -> Observable<Option<BzBusView>> {
    source::switch_map(window.id().box_it(), |window_id| {
        combine_latest!(
            bzbus_snapshots(),
            invocation_ids_for_window(window_id)
                => |(snapshot, invocation_ids)| {
                    if invocation_ids.is_empty() {
                        return None;
                    }

                    let invocations = snapshot
                        .invocations
                        .into_iter()
                        .filter(|invocation| invocation_ids.iter().any(|id| id == &invocation.id))
                        .collect();
                    Some(view::view(snapshot.active, invocations))
                },
        )
        .distinct_until_changed()
        .box_it()
    })
    .distinct_until_changed()
    .box_it()
}

pub(in crate::widgets::bar) fn ongoing_builds() -> Observable<Vec<BzBusView>> {
    bzbus_snapshots()
        .map(|snapshot| view::ongoing_views(snapshot.active, snapshot.invocations))
        .distinct_until_changed()
        .box_it()
}

fn bzbus_snapshots() -> Observable<BzBusSnapshot> {
    source::shared_by_key("rsynapse.bzbus-snapshots", BZBUS_OBJECT_PATH, || {
        source::from_task(|sender| async move {
            run_bzbus_source(sender).await;
        })
        .distinct_until_changed()
        .box_it()
    })
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
            if !emit_view(&sender, &mut latest, false, Vec::new()).await {
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
                if !emit_view(&sender, &mut latest, false, Vec::new()).await {
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

    if !emit_view(
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
                        if !emit_view(sender, latest, false, Vec::new()).await {
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
                        if !emit_view(sender, latest, true, invocations.values().cloned().collect()).await {
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
                        if !emit_view(sender, latest, true, invocations.values().cloned().collect()).await {
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

async fn emit_view(
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

fn invocation_ids_for_window(window_id: String) -> Observable<Vec<String>> {
    let subject = RelationEndpoint::stable_key(keys::WINDOW_ID, window_id);
    locus_targets(subject, WINDOW_BUILD_INVOCATION_RELATION)
        .map(|targets| {
            targets
                .into_iter()
                .filter_map(invocation_id_from_target)
                .collect()
        })
        .distinct_until_changed()
        .box_it()
}

fn invocation_id_from_target(target: RelationEndpoint) -> Option<String> {
    let RelationEndpoint::StableKey { kind, id } = target else {
        return None;
    };
    (kind == keys::BAZEL_INVOCATION_ID).then_some(id)
}

fn locus_targets(
    subject: RelationEndpoint,
    relation: &'static str,
) -> Observable<Vec<RelationEndpoint>> {
    source::watch_relation_subject(subject, relation)
        .map(|records| records.into_iter().map(|record| record.target).collect())
        .distinct_until_changed()
        .box_it()
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

enum WatchExit {
    Restart,
    Closed,
}
