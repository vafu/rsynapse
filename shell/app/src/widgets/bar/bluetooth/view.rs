use zbus::zvariant::OwnedObjectPath;

use super::{
    BluetoothDeviceGroup, BluetoothDeviceView, BluetoothStatusView, BluetoothView, DeviceGroupView,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AdapterSnapshot {
    path: OwnedObjectPath,
    powered: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DeviceSnapshot {
    view: BluetoothDeviceView,
    group: Option<BluetoothDeviceGroup>,
}

pub(super) fn adapter_snapshot(path: OwnedObjectPath, powered: bool) -> AdapterSnapshot {
    AdapterSnapshot { path, powered }
}

pub(super) fn device_snapshot(
    path: OwnedObjectPath,
    address: String,
    alias: Option<String>,
    name: Option<String>,
    icon: Option<String>,
    class: Option<u32>,
    connected: bool,
    connecting: bool,
    battery: Option<u8>,
) -> DeviceSnapshot {
    let name = display_name(alias, name, &address);
    let bluez_icon = present_string(icon);
    let class = class.filter(|class| *class != 0);
    let group = device_group(bluez_icon.as_deref(), class, &name);

    DeviceSnapshot {
        view: BluetoothDeviceView {
            path,
            name,
            address,
            icon: device_icon(group, bluez_icon.as_deref()).to_owned(),
            connected,
            connecting,
            battery,
        },
        group,
    }
}

pub(super) fn bluetooth_view(
    adapters: Vec<AdapterSnapshot>,
    mut devices: Vec<DeviceSnapshot>,
) -> BluetoothView {
    let adapter = adapters.into_iter().next();
    devices.sort_by(|a, b| {
        b.view
            .connected
            .cmp(&a.view.connected)
            .then_with(|| a.view.name.cmp(&b.view.name))
    });

    let powered = adapter
        .as_ref()
        .map(|adapter| adapter.powered)
        .unwrap_or(false);
    let connected_count = devices
        .iter()
        .filter(|device| device.view.connected)
        .count()
        .min(u8::MAX as usize) as u8;

    BluetoothView {
        status: BluetoothStatusView {
            icon: status_icon(powered, connected_count).to_owned(),
            connected_count,
            powered,
            adapter_path: adapter.map(|adapter| adapter.path),
        },
        keyboard: group_view(BluetoothDeviceGroup::Keyboard, &devices),
        audio: group_view(BluetoothDeviceGroup::Audio, &devices),
        pointer: group_view(BluetoothDeviceGroup::Pointer, &devices),
    }
}

fn group_view(group: BluetoothDeviceGroup, devices: &[DeviceSnapshot]) -> DeviceGroupView {
    let group_devices = devices
        .iter()
        .filter(|device| device.group == Some(group))
        .map(|device| device.view.clone())
        .collect::<Vec<_>>();
    let connected = group_devices
        .iter()
        .any(|device| device.connected || device.connecting);
    let battery = group_devices
        .iter()
        .filter(|device| device.connected)
        .filter_map(|device| device.battery)
        .next();

    DeviceGroupView {
        visible: !group_devices.is_empty(),
        icon: group_icon(group).to_owned(),
        tinted: connected,
        tooltip: group_tooltip(group, &group_devices),
        battery,
        devices: group_devices,
    }
}

fn group_tooltip(group: BluetoothDeviceGroup, devices: &[BluetoothDeviceView]) -> String {
    let label = group_label(group);
    if devices.is_empty() {
        return label.to_owned();
    }

    let connected = devices
        .iter()
        .filter(|device| device.connected)
        .map(|device| device.name.as_str())
        .collect::<Vec<_>>();
    if connected.is_empty() {
        format!(
            "{label}: {}",
            devices
                .iter()
                .map(|device| device.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        format!("{label}: {}", connected.join(", "))
    }
}

fn status_icon(powered: bool, connected_count: u8) -> &'static str {
    if !powered {
        "bluetooth_disabled"
    } else if connected_count > 0 {
        "bluetooth_connected"
    } else {
        "bluetooth"
    }
}

fn group_icon(group: BluetoothDeviceGroup) -> &'static str {
    match group {
        BluetoothDeviceGroup::Keyboard => "keyboard",
        BluetoothDeviceGroup::Audio => "headphones",
        BluetoothDeviceGroup::Pointer => "mouse",
    }
}

fn group_label(group: BluetoothDeviceGroup) -> &'static str {
    match group {
        BluetoothDeviceGroup::Keyboard => "Bluetooth keyboard",
        BluetoothDeviceGroup::Audio => "Bluetooth audio",
        BluetoothDeviceGroup::Pointer => "Bluetooth pointer",
    }
}

fn device_icon(group: Option<BluetoothDeviceGroup>, bluez_icon: Option<&str>) -> &'static str {
    match group {
        Some(group) => group_icon(group),
        None => {
            if bluez_icon
                .map(|icon| icon.contains("phone"))
                .unwrap_or(false)
            {
                "smartphone"
            } else {
                "bluetooth"
            }
        }
    }
}

fn device_group(
    icon: Option<&str>,
    class: Option<u32>,
    name: &str,
) -> Option<BluetoothDeviceGroup> {
    let icon = icon.unwrap_or_default().to_ascii_lowercase();
    let name = name.to_ascii_lowercase();
    let text = format!("{icon} {name}");

    if text.contains("keyboard") {
        return Some(BluetoothDeviceGroup::Keyboard);
    }
    if text.contains("mouse")
        || text.contains("pointer")
        || text.contains("touchpad")
        || text.contains("tablet")
    {
        return Some(BluetoothDeviceGroup::Pointer);
    }
    if text.contains("audio")
        || text.contains("headphone")
        || text.contains("headset")
        || text.contains("speaker")
        || text.contains("earbud")
    {
        return Some(BluetoothDeviceGroup::Audio);
    }

    class.and_then(device_group_from_class)
}

fn device_group_from_class(class: u32) -> Option<BluetoothDeviceGroup> {
    match class & 0x1f00 {
        0x0400 => Some(BluetoothDeviceGroup::Audio),
        0x0500 if class & 0x0040 != 0 => Some(BluetoothDeviceGroup::Keyboard),
        0x0500 if class & 0x0080 != 0 => Some(BluetoothDeviceGroup::Pointer),
        _ => None,
    }
}

fn display_name(alias: Option<String>, name: Option<String>, address: &str) -> String {
    present_string(alias)
        .or_else(|| present_string(name))
        .unwrap_or_else(|| address.to_owned())
}

fn present_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{BluetoothDeviceGroup, device_group, status_icon};

    #[test]
    fn status_icon_tracks_power_and_connections() {
        assert_eq!(status_icon(false, 0), "bluetooth_disabled");
        assert_eq!(status_icon(true, 0), "bluetooth");
        assert_eq!(status_icon(true, 2), "bluetooth_connected");
    }

    #[test]
    fn device_group_uses_bluez_icon_and_class() {
        assert_eq!(
            device_group(Some("input-keyboard"), None, "Keyboard"),
            Some(BluetoothDeviceGroup::Keyboard)
        );
        assert_eq!(
            device_group(Some("audio-headphones"), None, "Headphones"),
            Some(BluetoothDeviceGroup::Audio)
        );
        assert_eq!(
            device_group(None, Some(0x0540), "Peripheral"),
            Some(BluetoothDeviceGroup::Keyboard)
        );
    }
}
