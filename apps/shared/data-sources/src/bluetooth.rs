//! BlueZ adapter/device discovery and connection control.
use std::collections::HashMap;

use zbus::{
    blocking::{Connection, Proxy},
    names::OwnedInterfaceName,
    zvariant::{OwnedObjectPath, OwnedValue},
};

use crate::{
    Observable,
    dbus::{
        self, Bus, DbusInterface, DbusObject, ObjectDescriptor, ObjectManagerDescriptor,
        PropertyDescriptor,
    },
};

pub const BLUEZ_BUS: &str = "org.bluez";
pub const BLUEZ_OBJECT_PATH: &str = "/";
pub const ADAPTER_INTERFACE: &str = "org.bluez.Adapter1";
pub const DEVICE_INTERFACE: &str = "org.bluez.Device1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothAdapter {
    path: OwnedObjectPath,
    initial_powered: Option<bool>,
}

impl BluetoothAdapter {
    pub fn path(&self) -> OwnedObjectPath {
        self.path.clone()
    }

    pub fn path_string(&self) -> String {
        self.path.as_str().to_owned()
    }

    pub fn powered(&self) -> Observable<bool> {
        property_or(self.object(), "Powered", self.initial_powered, false)
    }

    fn from_object(object: &DbusObject) -> Self {
        Self {
            path: object.path.clone(),
            initial_powered: snapshot_property(object, ADAPTER_INTERFACE, "Powered"),
        }
    }

    fn object(&self) -> ObjectDescriptor {
        object(self.path.as_str(), ADAPTER_INTERFACE)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothDevice {
    path: OwnedObjectPath,
    initial_connected: Option<bool>,
}

impl BluetoothDevice {
    pub fn path(&self) -> OwnedObjectPath {
        self.path.clone()
    }

    pub fn connected(&self) -> Observable<bool> {
        property_or(self.object(), "Connected", self.initial_connected, false)
    }

    fn from_object(object: &DbusObject) -> Self {
        Self {
            path: object.path.clone(),
            initial_connected: snapshot_property(object, DEVICE_INTERFACE, "Connected"),
        }
    }

    fn object(&self) -> ObjectDescriptor {
        object(self.path.as_str(), DEVICE_INTERFACE)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothDeviceInfo {
    pub path: String,
    pub name: String,
    pub address: String,
    pub icon: String,
    pub connected: bool,
    pub connecting: bool,
}

impl BluetoothDeviceInfo {
    pub fn subtitle(&self) -> String {
        if self.connected {
            "Connected".to_owned()
        } else if self.connecting {
            "Connecting...".to_owned()
        } else {
            self.address.clone()
        }
    }

    pub fn symbolic_icon(&self) -> String {
        symbolic_icon(Some(&self.icon), self.connected)
    }
}

pub fn object_manager() -> ObjectManagerDescriptor {
    ObjectManagerDescriptor::parse(Bus::System, BLUEZ_BUS, BLUEZ_OBJECT_PATH)
        .expect("BlueZ descriptor should be valid")
}

pub fn models(objects: Vec<DbusObject>) -> (Vec<BluetoothAdapter>, Vec<BluetoothDevice>) {
    let mut adapters = Vec::new();
    let mut devices = Vec::new();
    for object in &objects {
        if has_interface(object, ADAPTER_INTERFACE) {
            adapters.push(BluetoothAdapter::from_object(object));
        }
        if has_interface(object, DEVICE_INTERFACE) {
            devices.push(BluetoothDevice::from_object(object));
        }
    }
    (adapters, devices)
}

pub fn set_adapter_power(path: Option<OwnedObjectPath>, powered: bool) {
    let Some(path) = path else {
        eprintln!("[bluetooth] cannot toggle power: no BlueZ adapter found");
        return;
    };
    let path = path.to_string();
    crate::write::spawn("bluetooth adapter power", async move {
        let connection = crate::write::system_connection().await?;
        let proxy =
            zbus::Proxy::new(connection, BLUEZ_BUS, path.as_str(), ADAPTER_INTERFACE).await?;
        proxy.set_property("Powered", &powered).await
    });
}

type Interfaces = HashMap<OwnedInterfaceName, HashMap<String, OwnedValue>>;
type ManagedObjects = HashMap<OwnedObjectPath, Interfaces>;

/// Blocking BlueZ device snapshot for launcher plugins.
pub fn devices_blocking() -> Result<Vec<BluetoothDeviceInfo>, String> {
    let connection = Connection::system().map_err(|error| format!("system bus: {error}"))?;
    let proxy = Proxy::new(
        &connection,
        BLUEZ_BUS,
        BLUEZ_OBJECT_PATH,
        "org.freedesktop.DBus.ObjectManager",
    )
    .map_err(|error| format!("BlueZ object manager: {error}"))?;
    let objects: ManagedObjects = proxy
        .call("GetManagedObjects", &())
        .map_err(|error| format!("GetManagedObjects: {error}"))?;

    let mut devices = objects
        .iter()
        .filter_map(|(path, interfaces)| device_info(path, interfaces))
        .collect::<Vec<_>>();
    devices.sort_by(|left, right| {
        right
            .connected
            .cmp(&left.connected)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(devices)
}

/// Blocking connect/disconnect for launcher plugins.
pub fn toggle_device_blocking(path: &str, connected: bool) -> Result<(), String> {
    let connection = Connection::system().map_err(|error| format!("system bus: {error}"))?;
    let proxy = Proxy::new(&connection, BLUEZ_BUS, path, DEVICE_INTERFACE)
        .map_err(|error| format!("BlueZ device {path}: {error}"))?;
    let method = if connected { "Disconnect" } else { "Connect" };
    let _: () = proxy
        .call(method, &())
        .map_err(|error| format!("{method} {path}: {error}"))?;
    Ok(())
}

pub fn symbolic_icon(bluez_icon: Option<&str>, connected: bool) -> String {
    let icon = match bluez_icon.unwrap_or_default() {
        "audio-headphones" | "audio-headset" => "audio-headphones-symbolic",
        "audio-speakers" | "audio-card" => "audio-speakers-symbolic",
        "input-keyboard" => "input-keyboard-symbolic",
        "input-mouse" => "input-mouse-symbolic",
        "input-gaming" => "input-gaming-symbolic",
        "input-tablet" => "input-tablet-symbolic",
        "phone" => "phone-symbolic",
        "computer" => "computer-symbolic",
        "camera-photo" => "camera-photo-symbolic",
        "camera-video" => "camera-video-symbolic",
        "printer" => "printer-symbolic",
        "scanner" => "scanner-symbolic",
        "video-display" => "video-display-symbolic",
        "multimedia-player" => "multimedia-player-symbolic",
        "network-wireless" => "network-wireless-symbolic",
        _ if connected => "bluetooth-active-symbolic",
        _ => "bluetooth-symbolic",
    };
    icon.to_owned()
}

fn device_info(path: &OwnedObjectPath, interfaces: &Interfaces) -> Option<BluetoothDeviceInfo> {
    let props = interfaces
        .iter()
        .find(|(name, _)| name.as_str() == DEVICE_INTERFACE)
        .map(|(_, props)| props)?;
    let address = string_property(props, "Address")?;
    let name = string_property(props, "Alias")
        .or_else(|| string_property(props, "Name"))
        .unwrap_or_else(|| address.clone());
    let connected = bool_property(props, "Connected").unwrap_or(false);
    let icon = symbolic_icon(string_property(props, "Icon").as_deref(), connected);
    Some(BluetoothDeviceInfo {
        path: path.as_str().to_owned(),
        name,
        address,
        icon,
        connected,
        connecting: bool_property(props, "Connecting").unwrap_or(false),
    })
}

fn string_property(props: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    props.get(key)?.try_clone().ok()?.try_into().ok()
}

fn bool_property(props: &HashMap<String, OwnedValue>, key: &str) -> Option<bool> {
    props.get(key)?.try_clone().ok()?.try_into().ok()
}

fn property_or<T>(
    object: ObjectDescriptor,
    property_name: &'static str,
    initial: Option<T>,
    default: T,
) -> Observable<T>
where
    T: TryFrom<OwnedValue> + Clone + PartialEq + Send + Sync + 'static,
    T::Error: std::fmt::Display,
{
    dbus::property_or_with_initial(
        PropertyDescriptor::new(object, property_name),
        initial,
        default,
    )
}

fn object(path: &str, interface: &'static str) -> ObjectDescriptor {
    ObjectDescriptor::parse(Bus::System, BLUEZ_BUS, path, interface)
        .expect("BlueZ descriptor should be valid")
}

fn has_interface(object: &DbusObject, interface_name: &str) -> bool {
    interface(object, interface_name).is_some()
}

fn snapshot_property<T>(object: &DbusObject, interface_name: &str, property_name: &str) -> Option<T>
where
    T: TryFrom<OwnedValue>,
    T::Error: std::fmt::Display,
{
    let property = interface(object, interface_name)?
        .properties
        .iter()
        .find(|property| property.name == property_name)?;
    let value = match property.value.as_ref().try_clone() {
        Ok(value) => value,
        Err(error) => {
            eprintln!(
                "[bluetooth] clone initial BlueZ property {}:{} failed: {error}",
                object.path, property_name
            );
            return None;
        }
    };
    match T::try_from(value) {
        Ok(value) => Some(value),
        Err(error) => {
            eprintln!(
                "[bluetooth] decode initial BlueZ property {}:{} failed: {error}",
                object.path, property_name
            );
            None
        }
    }
}

fn interface<'a>(object: &'a DbusObject, interface_name: &str) -> Option<&'a DbusInterface> {
    object
        .interfaces
        .iter()
        .find(|interface| interface.name.as_str() == interface_name)
}

#[cfg(test)]
mod tests {
    use super::symbolic_icon;

    #[test]
    fn device_icons_are_symbolic() {
        assert_eq!(
            symbolic_icon(Some("audio-headphones"), false),
            "audio-headphones-symbolic"
        );
        assert_eq!(
            symbolic_icon(Some("input-keyboard"), true),
            "input-keyboard-symbolic"
        );
        assert_eq!(
            symbolic_icon(Some("unknown"), true),
            "bluetooth-active-symbolic"
        );
        assert_eq!(symbolic_icon(None, false), "bluetooth-symbolic");
    }
}
