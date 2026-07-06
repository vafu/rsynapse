use shell_core::source::{
    Observable,
    dbus::{self, Bus, ObjectDescriptor, ObjectManagerDescriptor, ObjectModel, PropertyDescriptor},
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
}

impl BluezAdapter {
    pub(super) fn path(&self) -> OwnedObjectPath {
        self.path.clone()
    }

    pub(super) fn powered(&self) -> Observable<bool> {
        property_or(self.object(), "Powered", false)
    }

    fn object(&self) -> ObjectDescriptor {
        object(self.path.as_str(), ADAPTER_INTERFACE)
    }
}

impl ObjectModel for BluezAdapter {
    const INTERFACE: &'static str = ADAPTER_INTERFACE;

    fn at(path: OwnedObjectPath) -> Self {
        Self { path }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct BluezDevice {
    path: OwnedObjectPath,
}

impl BluezDevice {
    pub(super) fn path(&self) -> OwnedObjectPath {
        self.path.clone()
    }

    pub(super) fn address(&self) -> Observable<String> {
        property_or(self.device_object(), "Address", String::new())
    }

    pub(super) fn alias(&self) -> Observable<Option<String>> {
        property(self.device_object(), "Alias")
    }

    pub(super) fn name(&self) -> Observable<Option<String>> {
        property(self.device_object(), "Name")
    }

    pub(super) fn icon(&self) -> Observable<Option<String>> {
        property(self.device_object(), "Icon")
    }

    pub(super) fn class(&self) -> Observable<Option<u32>> {
        property(self.device_object(), "Class")
    }

    pub(super) fn connected(&self) -> Observable<bool> {
        property_or(self.device_object(), "Connected", false)
    }

    pub(super) fn connecting(&self) -> Observable<bool> {
        property_or(self.device_object(), "Connecting", false)
    }

    pub(super) fn battery_percentage(&self) -> Observable<Option<u8>> {
        property(self.battery_object(), "Percentage")
    }

    fn device_object(&self) -> ObjectDescriptor {
        object(self.path.as_str(), DEVICE_INTERFACE)
    }

    fn battery_object(&self) -> ObjectDescriptor {
        object(self.path.as_str(), BATTERY_INTERFACE)
    }
}

impl ObjectModel for BluezDevice {
    const INTERFACE: &'static str = DEVICE_INTERFACE;

    fn at(path: OwnedObjectPath) -> Self {
        Self { path }
    }
}

pub(super) fn bluez() -> ObjectManagerDescriptor {
    ObjectManagerDescriptor::parse(Bus::System, BLUEZ_BUS, BLUEZ_OBJECT_PATH)
        .expect("BlueZ descriptor should be valid")
}

fn property<T>(object: ObjectDescriptor, property_name: &'static str) -> Observable<Option<T>>
where
    T: TryFrom<OwnedValue> + Clone + PartialEq + Send + 'static,
    T::Error: std::fmt::Display,
{
    dbus::property(PropertyDescriptor::new(object, property_name))
}

fn property_or<T>(
    object: ObjectDescriptor,
    property_name: &'static str,
    default: T,
) -> Observable<T>
where
    T: TryFrom<OwnedValue> + Clone + PartialEq + Send + 'static,
    T::Error: std::fmt::Display,
{
    dbus::property_or(PropertyDescriptor::new(object, property_name), default)
}

fn object(path: &str, interface: &'static str) -> ObjectDescriptor {
    ObjectDescriptor::parse(Bus::System, BLUEZ_BUS, path, interface)
        .expect("BlueZ descriptor should be valid")
}
