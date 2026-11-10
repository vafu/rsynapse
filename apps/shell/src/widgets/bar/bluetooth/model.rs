pub(super) use data_sources::bluetooth::{BluetoothAdapter, BluetoothDevice};

pub(super) fn bluez() -> data_sources::dbus::ObjectManagerDescriptor {
    data_sources::bluetooth::object_manager()
}

pub(super) fn bluez_models(
    objects: Vec<data_sources::dbus::DbusObject>,
) -> (Vec<BluetoothAdapter>, Vec<BluetoothDevice>) {
    data_sources::bluetooth::models(objects)
}
