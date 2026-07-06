use shell_core::source::{
    self, Observable,
    dbus::{self, Bus, ObjectDescriptor, ObjectManagerDescriptor, ObjectModel, PropertyDescriptor},
    rx::Observable as _,
};
use shell_rx_macros::combine_latest;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const NM_BUS: &str = "org.freedesktop.NetworkManager";
const NM_OBJECT_MANAGER_PATH: &str = "/org/freedesktop";
const DEVICE_INTERFACE: &str = "org.freedesktop.NetworkManager.Device";
const WIRELESS_INTERFACE: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const ACCESS_POINT_INTERFACE: &str = "org.freedesktop.NetworkManager.AccessPoint";

const DEVICE_TYPE_ETHERNET: u32 = 1;
const DEVICE_TYPE_WIFI: u32 = 2;

#[derive(Clone, Debug, Eq, PartialEq)]
struct NetworkDevice {
    path: OwnedObjectPath,
}

impl NetworkDevice {
    fn interface(&self) -> Observable<String> {
        property_or(self.object(DEVICE_INTERFACE), "Interface", String::new())
    }

    fn state(&self) -> Observable<u32> {
        property_or(self.object(DEVICE_INTERFACE), "State", 0)
    }

    fn device_type(&self) -> Observable<u32> {
        property_or(self.object(DEVICE_INTERFACE), "DeviceType", 0)
    }

    fn active_access_point(&self) -> Observable<Option<AccessPoint>> {
        property_or(
            self.object(WIRELESS_INTERFACE),
            "ActiveAccessPoint",
            root_path(),
        )
        .map(|path: OwnedObjectPath| (path.as_str() != "/").then(|| AccessPoint { path }))
        .distinct_until_changed()
        .box_it()
    }

    fn object(&self, interface: &'static str) -> ObjectDescriptor {
        object(self.path.as_str(), interface)
    }
}

impl ObjectModel for NetworkDevice {
    const INTERFACE: &'static str = DEVICE_INTERFACE;

    fn at(path: OwnedObjectPath) -> Self {
        Self { path }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AccessPoint {
    path: OwnedObjectPath,
}

impl AccessPoint {
    fn snapshot(&self) -> Observable<AccessPointSnapshot> {
        combine_latest!(
            property_or::<Vec<u8>>(self.object(), "Ssid", Vec::new()).map(ssid),
            property_or::<u8>(self.object(), "Strength", 0)
                => |(ssid, strength)| AccessPointSnapshot { ssid, strength },
        )
        .distinct_until_changed()
        .box_it()
    }

    fn object(&self) -> ObjectDescriptor {
        object(self.path.as_str(), ACCESS_POINT_INTERFACE)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NetworkDeviceBase {
    interface: String,
    state: u32,
    kind: Option<NetworkKind>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NetworkDeviceSnapshot {
    pub(super) interface: String,
    pub(super) state: u32,
    pub(super) kind: NetworkKind,
    pub(super) access_point: Option<AccessPointSnapshot>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NetworkKind {
    Wifi,
    Ethernet,
}

impl NetworkKind {
    fn from_device_type(device_type: u32) -> Option<Self> {
        match device_type {
            DEVICE_TYPE_WIFI => Some(Self::Wifi),
            DEVICE_TYPE_ETHERNET => Some(Self::Ethernet),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AccessPointSnapshot {
    pub(super) ssid: Option<String>,
    pub(super) strength: u8,
}

pub(super) fn network_devices() -> Observable<Vec<Option<NetworkDeviceSnapshot>>> {
    source::switch_map_list(
        dbus::models::<NetworkDevice>(network_manager()),
        device_snapshot,
    )
    .distinct_until_changed()
    .box_it()
}

fn device_snapshot(device: NetworkDevice) -> Observable<Option<NetworkDeviceSnapshot>> {
    let base = combine_latest!(
        device.interface(),
        device.state(),
        device.device_type().map(NetworkKind::from_device_type)
            => |(interface, state, kind)| NetworkDeviceBase {
                interface,
                state,
                kind,
            },
    )
    .box_it();

    source::switch_map(
        base,
        move |base: NetworkDeviceBase| -> Observable<Option<NetworkDeviceSnapshot>> {
            let device = device.clone();
            match base.kind {
                Some(NetworkKind::Wifi) => wifi_snapshot(device, base),
                Some(NetworkKind::Ethernet) => source::once(Some(NetworkDeviceSnapshot {
                    interface: base.interface,
                    state: base.state,
                    kind: NetworkKind::Ethernet,
                    access_point: None,
                })),
                None => source::once(None),
            }
        },
    )
    .distinct_until_changed()
    .box_it()
}

fn wifi_snapshot(
    device: NetworkDevice,
    base: NetworkDeviceBase,
) -> Observable<Option<NetworkDeviceSnapshot>> {
    source::switch_map(
        device.active_access_point(),
        move |access_point: Option<AccessPoint>| -> Observable<Option<NetworkDeviceSnapshot>> {
            let base = base.clone();
            match access_point {
                Some(access_point) => access_point
                    .snapshot()
                    .map(move |access_point| {
                        Some(NetworkDeviceSnapshot {
                            interface: base.interface.clone(),
                            state: base.state,
                            kind: NetworkKind::Wifi,
                            access_point: Some(access_point),
                        })
                    })
                    .box_it(),
                None => source::once(Some(NetworkDeviceSnapshot {
                    interface: base.interface,
                    state: base.state,
                    kind: NetworkKind::Wifi,
                    access_point: None,
                })),
            }
        },
    )
    .box_it()
}

fn network_manager() -> ObjectManagerDescriptor {
    ObjectManagerDescriptor::parse(Bus::System, NM_BUS, NM_OBJECT_MANAGER_PATH)
        .expect("NetworkManager descriptor should be valid")
}

fn ssid(bytes: Vec<u8>) -> Option<String> {
    let name = String::from_utf8_lossy(&bytes)
        .trim_end_matches('\0')
        .trim()
        .to_owned();
    (!name.is_empty()).then_some(name)
}

fn property_or<T>(object: ObjectDescriptor, property: &'static str, default: T) -> Observable<T>
where
    T: TryFrom<OwnedValue> + Clone + PartialEq + Send + 'static,
    T::Error: std::fmt::Display,
{
    dbus::property_or(PropertyDescriptor::new(object, property), default)
}

fn object(path: &str, interface: &'static str) -> ObjectDescriptor {
    ObjectDescriptor::parse(Bus::System, NM_BUS, path, interface)
        .expect("NetworkManager descriptor should be valid")
}

fn root_path() -> OwnedObjectPath {
    OwnedObjectPath::try_from("/").expect("root path should be valid")
}

#[cfg(test)]
mod tests {
    use super::ssid;

    #[test]
    fn ssid_discards_empty_names() {
        assert_eq!(ssid(b"My Wifi\0".to_vec()), Some("My Wifi".to_owned()));
        assert_eq!(ssid(Vec::new()), None);
    }
}
