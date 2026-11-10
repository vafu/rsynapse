//! UI-free reactive client for the Mist device-bridge D-Bus service.
//!
//! The service exposes one flat `Devices` array property. List surfaces must not
//! rebuild rows from that snapshot; instead this module projects it into a
//! structural key list ([`device_keys`]) plus per-key lookups ([`device`]), the
//! same split used by `github::{activity, activity_entry}`.

use crate::{
    Observable,
    dbus::{self, Bus, ObjectDescriptor, PropertyDescriptor},
    rx::Observable as _,
};

/// Well-known session bus name owned by `mistd`.
pub const BUS_NAME: &str = "io.github.vafu.Mist";
/// Root object exported by `mistd`.
pub const OBJECT_PATH: &str = "/io/github/vafu/Mist";
const INTERFACE: &str = "io.github.vafu.Mist1";

/// Serialized D-Bus device row: endpoint ID, display name, device class,
/// platform, status, transport, selected.
pub type DeviceRow = (String, String, String, String, String, String, bool);

/// Connection state of a paired device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceStatus {
    Connected,
    Connecting,
    Disconnected,
}

/// Physical form of a paired device, as declared by the peer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceClass {
    Phone,
    Tablet,
    Laptop,
    Desktop,
    Server,
    Other,
}

/// Transport Mist currently uses for a connected device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionTransport {
    Direct,
    Relay,
    Custom,
    Unknown,
}

impl ConnectionTransport {
    fn as_str(self) -> Option<&'static str> {
        match self {
            Self::Direct => Some("direct"),
            Self::Relay => Some("relay"),
            Self::Custom => Some("custom"),
            Self::Unknown => None,
        }
    }
}

/// One paired device as reported by the Mist service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MistDevice {
    /// Stable endpoint identity; the row key for list reconciliation.
    pub endpoint_id: String,
    pub name: String,
    pub class: DeviceClass,
    pub platform: String,
    pub status: DeviceStatus,
    pub transport: ConnectionTransport,
    pub selected: bool,
}

/// Whether `mistd` currently owns the session bus name.
pub fn available() -> Observable<bool> {
    devices()
        .map(|devices| devices.is_some())
        .distinct_until_changed()
        .box_it()
}

/// Whether at least one paired device is connected.
pub fn connected() -> Observable<bool> {
    crate::shared_by_key("mist-connected", OBJECT_PATH, || {
        crate::switch_map(
            dbus::service_owner(Bus::Session, BUS_NAME),
            |owner| match owner {
                None => crate::once(false),
                Some(owner) => {
                    dbus::property::<bool>(PropertyDescriptor::new(object(&owner), "Connected"))
                        .map(|value| value.unwrap_or(false))
                        .start_with(vec![false])
                        .box_it()
                }
            },
        )
        .distinct_until_changed()
        .box_it()
    })
}

/// Full paired-device snapshot, or `None` while the service is unavailable.
///
/// This is a data snapshot, not a list-reconciliation trigger. Use
/// [`device_keys`] for structure and [`device`] for per-row data.
pub fn devices() -> Observable<Option<Vec<MistDevice>>> {
    crate::shared_by_key("mist-devices", OBJECT_PATH, || {
        crate::switch_map(
            dbus::service_owner(Bus::Session, BUS_NAME),
            |owner| match owner {
                None => crate::once(None),
                Some(owner) => dbus::property::<Vec<DeviceRow>>(PropertyDescriptor::new(
                    object(&owner),
                    "Devices",
                ))
                .map(|rows| rows.map(|rows| rows.into_iter().map(device_from_row).collect()))
                .start_with(vec![None])
                .box_it(),
            },
        )
        .distinct_until_changed()
        .box_it()
    })
}

/// Structural list of device keys for `#[bind_list]` reconciliation.
///
/// Changes only when a device is added or removed, never when a device's
/// status, name, or selection changes.
pub fn device_keys() -> Observable<Vec<String>> {
    devices()
        .map(|devices| {
            devices
                .map(|devices| {
                    devices
                        .into_iter()
                        .map(|device| device.endpoint_id)
                        .collect()
                })
                .unwrap_or_default()
        })
        .distinct_until_changed()
        .box_it()
}

/// Live view of one device, keyed by endpoint ID, for a single row's `#[source]`.
pub fn device(endpoint_id: String) -> Observable<Option<MistDevice>> {
    crate::shared_by_key("mist-device", endpoint_id.clone(), move || {
        let endpoint_id = endpoint_id.clone();
        devices()
            .map(move |devices| {
                devices.and_then(|devices| {
                    devices
                        .into_iter()
                        .find(|device| device.endpoint_id == endpoint_id)
                })
            })
            .distinct_until_changed()
            .box_it()
    })
}

fn object(owner: &str) -> ObjectDescriptor {
    ObjectDescriptor::parse(Bus::Session, owner, OBJECT_PATH, INTERFACE)
        .expect("Mist descriptor should be valid")
}

fn device_from_row(
    (endpoint_id, name, class, platform, status, transport, selected): DeviceRow,
) -> MistDevice {
    MistDevice {
        endpoint_id,
        name,
        class: parse_class(&class),
        platform,
        status: parse_status(&status),
        transport: parse_transport(&transport),
        selected,
    }
}

fn parse_class(class: &str) -> DeviceClass {
    match class {
        "phone" => DeviceClass::Phone,
        "tablet" => DeviceClass::Tablet,
        "laptop" => DeviceClass::Laptop,
        "desktop" => DeviceClass::Desktop,
        "server" => DeviceClass::Server,
        _ => DeviceClass::Other,
    }
}

fn parse_status(status: &str) -> DeviceStatus {
    match status {
        "connected" => DeviceStatus::Connected,
        "connecting" => DeviceStatus::Connecting,
        _ => DeviceStatus::Disconnected,
    }
}

fn parse_transport(transport: &str) -> ConnectionTransport {
    match transport {
        "direct" => ConnectionTransport::Direct,
        "relay" => ConnectionTransport::Relay,
        "custom" => ConnectionTransport::Custom,
        _ => ConnectionTransport::Unknown,
    }
}

/// Human-readable transport suffix, e.g. `connected · relay`.
#[must_use]
pub fn transport_label(transport: ConnectionTransport) -> Option<&'static str> {
    transport.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(status: &str, transport: &str, selected: bool) -> DeviceRow {
        (
            "endpoint".to_owned(),
            "Pixel".to_owned(),
            "phone".to_owned(),
            "android".to_owned(),
            status.to_owned(),
            transport.to_owned(),
            selected,
        )
    }

    #[test]
    fn parses_status_and_transport() {
        let device = device_from_row(row("connected", "relay", true));
        assert_eq!(device.status, DeviceStatus::Connected);
        assert_eq!(device.transport, ConnectionTransport::Relay);
        assert_eq!(device.class, DeviceClass::Phone);
        assert!(device.selected);
        assert_eq!(device.platform, "android");
    }

    #[test]
    fn parses_each_declared_class() {
        for (raw, expected) in [
            ("phone", DeviceClass::Phone),
            ("tablet", DeviceClass::Tablet),
            ("laptop", DeviceClass::Laptop),
            ("desktop", DeviceClass::Desktop),
            ("server", DeviceClass::Server),
            ("other", DeviceClass::Other),
            ("mystery", DeviceClass::Other),
        ] {
            let row = (
                "id".to_owned(),
                "n".to_owned(),
                raw.to_owned(),
                "p".to_owned(),
                "connected".to_owned(),
                "relay".to_owned(),
                false,
            );
            assert_eq!(device_from_row(row).class, expected);
        }
    }

    #[test]
    fn unknown_values_degrade_safely() {
        let device = device_from_row(row("weird", "sneakernet", false));
        assert_eq!(device.status, DeviceStatus::Disconnected);
        assert_eq!(device.transport, ConnectionTransport::Unknown);
    }

    #[test]
    fn transport_labels_only_for_known_transports() {
        assert_eq!(transport_label(ConnectionTransport::Direct), Some("direct"));
        assert_eq!(transport_label(ConnectionTransport::Relay), Some("relay"));
        assert_eq!(transport_label(ConnectionTransport::Custom), Some("custom"));
        assert_eq!(transport_label(ConnectionTransport::Unknown), None);
    }
}
