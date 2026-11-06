use super::BluetoothView;
use super::model::{
    ADAPTER_INTERFACE, BLUEZ_BUS, BLUEZ_OBJECT_PATH, BluezAdapter, BluezDevice, bluez, bluez_models,
};
use super::view::{self, AdapterSnapshot};
use shell_core::source::{self, Observable, dbus, rx::Observable as _};
use shell_rx_macros::combine_latest;
use zbus::zvariant::OwnedObjectPath;

pub(super) fn bluetooth_status() -> Observable<BluetoothView> {
    source::shared_by_key("rsynapse.bluetooth-status", BLUEZ_OBJECT_PATH, || {
        let models = dbus::object_manager(bluez())
            .map(bluez_models)
            .distinct_until_changed()
            .box_it();

        source::switch_map(
            models,
            |(adapters, devices): (Vec<BluezAdapter>, Vec<BluezDevice>)| {
                combine_latest!(
                    source::switch_map_list(source::once(adapters), adapter_snapshot),
                    source::switch_map_list(source::once(devices), device_snapshot)
                        => |(adapters, devices)| view::bluetooth_view(adapters, devices),
                )
                .box_it()
            },
        )
        .distinct_until_changed()
        .box_it()
    })
}

pub(super) fn set_adapter_power(path: Option<OwnedObjectPath>, powered: bool) {
    let Some(path) = path else {
        eprintln!("[bluetooth] cannot toggle power: no BlueZ adapter found");
        return;
    };
    let path = path.to_string();
    shell_core::source::write::spawn("bluetooth adapter power", async move {
        let connection = shell_core::source::write::system_connection().await?;
        let proxy =
            zbus::Proxy::new(connection, BLUEZ_BUS, path.as_str(), ADAPTER_INTERFACE).await?;
        proxy.set_property("Powered", &powered).await
    });
}

fn adapter_snapshot(adapter: BluezAdapter) -> Observable<AdapterSnapshot> {
    let path = adapter.path();
    adapter
        .powered()
        .map(move |powered| view::adapter_snapshot(path.clone(), powered))
        .distinct_until_changed()
        .box_it()
}

fn device_snapshot(device: BluezDevice) -> Observable<bool> {
    device.connected().distinct_until_changed().box_it()
}
