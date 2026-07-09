use std::collections::HashMap;

use zbus::{
    Message,
    names::OwnedInterfaceName,
    zvariant::{OwnedObjectPath, OwnedValue},
};

use super::super::view::Invocation;

pub(super) const INVOCATION_INTERFACE: &str = "com.snap.BzBus.Invocation1";
const INTERFACES_ADDED: &str = "InterfacesAdded";
const INTERFACES_REMOVED: &str = "InterfacesRemoved";

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

pub(super) type PropertyMap = HashMap<String, OwnedValue>;
pub(super) type InterfaceMap = HashMap<OwnedInterfaceName, PropertyMap>;
pub(super) type RawManagedObjects = HashMap<OwnedObjectPath, InterfaceMap>;
pub(super) type InvocationMap = HashMap<OwnedObjectPath, Invocation>;

pub(super) fn invocations_from_managed_objects(objects: RawManagedObjects) -> InvocationMap {
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

pub(super) fn apply_object_manager_signal(
    invocations: &mut InvocationMap,
    message: Message,
) -> SourceChange {
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

pub(super) fn apply_properties_changed(
    invocations: &mut InvocationMap,
    message: Message,
) -> SourceChange {
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

pub(super) fn invocation_from_properties(
    path: &OwnedObjectPath,
    properties: &PropertyMap,
) -> Invocation {
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

pub(super) fn apply_invocation_properties(invocation: &mut Invocation, properties: &PropertyMap) {
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

pub(super) fn i64_value(value: &OwnedValue) -> Option<i64> {
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

pub(super) fn u32_value(value: &OwnedValue) -> Option<u32> {
    u32::try_from(value)
        .ok()
        .or_else(|| i64_value(value).and_then(|value| u32::try_from(value).ok()))
}

pub(super) fn u64_value(value: &OwnedValue) -> Option<u64> {
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

fn format_dbus_error(context: &str, error: impl std::fmt::Display) -> String {
    format!("{context} failed: {error}")
}

pub(super) enum SourceChange {
    Changed,
    Ignore,
    Error(String),
}
