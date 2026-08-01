use std::thread;

use super::model::{
    ADAPTER_INTERFACE, BLUEZ_BUS, BLUEZ_OBJECT_PATH, BluezAdapter, BluezDevice, DEVICE_INTERFACE,
    bluez, bluez_models,
};
use super::view::{self, AdapterSnapshot, DeviceSnapshot};
use super::{BluetoothDeviceGroup, BluetoothDeviceView, BluetoothView};
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

pub(super) fn bluetooth_group_devices(
    group: BluetoothDeviceGroup,
) -> Observable<Vec<BluetoothDeviceView>> {
    bluetooth_status()
        .map(move |view| match group {
            BluetoothDeviceGroup::Keyboard => view.keyboard.devices,
            BluetoothDeviceGroup::Audio => view.audio.devices,
            BluetoothDeviceGroup::Pointer => view.pointer.devices,
        })
        .distinct_until_changed()
        .box_it()
}

pub(super) fn set_adapter_power(path: Option<OwnedObjectPath>, powered: bool) {
    let Some(path) = path else {
        eprintln!("[bluetooth] cannot toggle power: no BlueZ adapter found");
        return;
    };
    let path = path.to_string();

    thread::spawn(move || {
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())
            .and_then(|runtime| {
                runtime.block_on(async move {
                    let connection = zbus::Connection::system()
                        .await
                        .map_err(|error| error.to_string())?;
                    let proxy =
                        zbus::Proxy::new(&connection, BLUEZ_BUS, path.as_str(), ADAPTER_INTERFACE)
                            .await
                            .map_err(|error| error.to_string())?;
                    proxy
                        .set_property("Powered", &powered)
                        .await
                        .map_err(|error| error.to_string())
                })
            });

        if let Err(error) = result {
            eprintln!("[bluetooth] failed to set adapter power: {error}");
        }
    });
}

pub(super) fn set_device_connected(path: OwnedObjectPath, connect: bool) {
    let path = path.to_string();
    let method = if connect { "Connect" } else { "Disconnect" };

    thread::spawn(move || {
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())
            .and_then(|runtime| {
                runtime.block_on(async move {
                    let connection = zbus::Connection::system()
                        .await
                        .map_err(|error| error.to_string())?;
                    let proxy =
                        zbus::Proxy::new(&connection, BLUEZ_BUS, path.as_str(), DEVICE_INTERFACE)
                            .await
                            .map_err(|error| error.to_string())?;
                    proxy
                        .call_method(method, &())
                        .await
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                })
            });

        if let Err(error) = result {
            eprintln!("[bluetooth] failed to call {method}: {error}");
        }
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

fn device_snapshot(device: BluezDevice) -> Observable<DeviceSnapshot> {
    let path = device.path();
    combine_latest!(
        device.address(),
        device.alias(),
        device.name(),
        device.icon(),
        device.class(),
        device.connected(),
        device.connecting(),
        device.battery_percentage()
            => move |(address, alias, name, icon, class, connected, connecting, battery)| {
                view::device_snapshot(
                    path.clone(),
                    address,
                    alias,
                    name,
                    icon,
                    class,
                    connected,
                    connecting,
                    battery,
                )
            },
    )
    .distinct_until_changed()
    .box_it()
}
