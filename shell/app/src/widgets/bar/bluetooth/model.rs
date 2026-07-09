use std::fmt;

use shell_core::source::{
    Observable,
    dbus::{
        self, Bus, DbusInterface, DbusObject, ObjectDescriptor, ObjectManagerDescriptor,
        PropertyDescriptor,
    },
};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

pub(super) const BLUEZ_BUS: &str = "org.bluez";
pub(super) const BLUEZ_OBJECT_PATH: &str = "/";
pub(super) const ADAPTER_INTERFACE: &str = "org.bluez.Adapter1";
pub(super) const DEVICE_INTERFACE: &str = "org.bluez.Device1";
const BATTERY_INTERFACE: &str = "org.bluez.Battery1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct BluezAdapter {
    path: OwnedObjectPath,
    initial_powered: Option<bool>,
}

impl BluezAdapter {
    pub(super) fn path(&self) -> OwnedObjectPath {
        self.path.clone()
    }

    pub(super) fn powered(&self) -> Observable<bool> {
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
pub(super) struct BluezDevice {
    path: OwnedObjectPath,
    initial_address: Option<String>,
    initial_alias: Option<String>,
    initial_name: Option<String>,
    initial_icon: Option<String>,
    initial_class: Option<u32>,
    initial_connected: Option<bool>,
    initial_connecting: Option<bool>,
    initial_battery_percentage: Option<u8>,
}

impl BluezDevice {
    pub(super) fn path(&self) -> OwnedObjectPath {
        self.path.clone()
    }

    pub(super) fn address(&self) -> Observable<String> {
        property_or(
            self.device_object(),
            "Address",
            self.initial_address.clone(),
            String::new(),
        )
    }

    pub(super) fn alias(&self) -> Observable<Option<String>> {
        property(self.device_object(), "Alias", self.initial_alias.clone())
    }

    pub(super) fn name(&self) -> Observable<Option<String>> {
        property(self.device_object(), "Name", self.initial_name.clone())
    }

    pub(super) fn icon(&self) -> Observable<Option<String>> {
        property(self.device_object(), "Icon", self.initial_icon.clone())
    }

    pub(super) fn class(&self) -> Observable<Option<u32>> {
        property(self.device_object(), "Class", self.initial_class)
    }

    pub(super) fn connected(&self) -> Observable<bool> {
        property_or(
            self.device_object(),
            "Connected",
            self.initial_connected,
            false,
        )
    }

    pub(super) fn connecting(&self) -> Observable<bool> {
        property_or(
            self.device_object(),
            "Connecting",
            self.initial_connecting,
            false,
        )
    }

    pub(super) fn battery_percentage(&self) -> Observable<Option<u8>> {
        property(
            self.battery_object(),
            "Percentage",
            self.initial_battery_percentage,
        )
    }

    fn from_object(object: &DbusObject) -> Self {
        Self {
            path: object.path.clone(),
            initial_address: snapshot_property(object, DEVICE_INTERFACE, "Address"),
            initial_alias: snapshot_property(object, DEVICE_INTERFACE, "Alias"),
            initial_name: snapshot_property(object, DEVICE_INTERFACE, "Name"),
            initial_icon: snapshot_property(object, DEVICE_INTERFACE, "Icon"),
            initial_class: snapshot_property(object, DEVICE_INTERFACE, "Class"),
            initial_connected: snapshot_property(object, DEVICE_INTERFACE, "Connected"),
            initial_connecting: snapshot_property(object, DEVICE_INTERFACE, "Connecting"),
            initial_battery_percentage: snapshot_property(object, BATTERY_INTERFACE, "Percentage"),
        }
    }

    fn device_object(&self) -> ObjectDescriptor {
        object(self.path.as_str(), DEVICE_INTERFACE)
    }

    fn battery_object(&self) -> ObjectDescriptor {
        object(self.path.as_str(), BATTERY_INTERFACE)
    }
}

pub(super) fn bluez() -> ObjectManagerDescriptor {
    ObjectManagerDescriptor::parse(Bus::System, BLUEZ_BUS, BLUEZ_OBJECT_PATH)
        .expect("BlueZ descriptor should be valid")
}

pub(super) fn bluez_models(objects: Vec<DbusObject>) -> (Vec<BluezAdapter>, Vec<BluezDevice>) {
    let mut adapters = Vec::new();
    let mut devices = Vec::new();

    for object in &objects {
        if has_interface(object, ADAPTER_INTERFACE) {
            adapters.push(BluezAdapter::from_object(object));
        }
        if has_interface(object, DEVICE_INTERFACE) {
            devices.push(BluezDevice::from_object(object));
        }
    }

    (adapters, devices)
}

fn property<T>(
    object: ObjectDescriptor,
    property_name: &'static str,
    initial: Option<T>,
) -> Observable<Option<T>>
where
    T: TryFrom<OwnedValue> + Clone + PartialEq + Send + Sync + 'static,
    T::Error: fmt::Display,
{
    dbus::property_with_initial(PropertyDescriptor::new(object, property_name), initial)
}

fn property_or<T>(
    object: ObjectDescriptor,
    property_name: &'static str,
    initial: Option<T>,
    default: T,
) -> Observable<T>
where
    T: TryFrom<OwnedValue> + Clone + PartialEq + Send + Sync + 'static,
    T::Error: fmt::Display,
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
    T::Error: fmt::Display,
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
