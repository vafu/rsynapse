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
    initial_connected: Option<bool>,
}

impl BluezDevice {
    pub(super) fn connected(&self) -> Observable<bool> {
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
