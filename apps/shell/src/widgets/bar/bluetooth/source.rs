use super::BluetoothView;
use super::model::{BluetoothAdapter, BluetoothDevice, bluez, bluez_models};
use super::view::{self, AdapterSnapshot};
use data_sources::zvariant::OwnedObjectPath;
use data_sources::{self, Observable, dbus, rx::Observable as _};
use shell_rx_macros::combine_latest;

pub(super) fn bluetooth_status() -> Observable<BluetoothView> {
    data_sources::shared_by_key(
        "rsynapse.bluetooth-status",
        data_sources::bluetooth::BLUEZ_OBJECT_PATH,
        || {
            let models = dbus::object_manager(bluez())
                .map(bluez_models)
                .distinct_until_changed()
                .box_it();

            data_sources::switch_map(
                models,
                |(adapters, devices): (Vec<BluetoothAdapter>, Vec<BluetoothDevice>)| {
                    combine_latest!(
                    data_sources::switch_map_list(data_sources::once(adapters), adapter_snapshot),
                    data_sources::switch_map_list(data_sources::once(devices), device_snapshot)
                        => |(adapters, devices)| view::bluetooth_view(adapters, devices),
                )
                .box_it()
                },
            )
            .distinct_until_changed()
            .box_it()
        },
    )
}

pub(super) fn set_adapter_power(path: Option<OwnedObjectPath>, powered: bool) {
    data_sources::bluetooth::set_adapter_power(path, powered);
}

fn adapter_snapshot(adapter: BluetoothAdapter) -> Observable<AdapterSnapshot> {
    let path = adapter.path();
    adapter
        .powered()
        .map(move |powered| view::adapter_snapshot(path.clone(), powered))
        .distinct_until_changed()
        .box_it()
}

fn device_snapshot(device: BluetoothDevice) -> Observable<bool> {
    device.connected().distinct_until_changed().box_it()
}
