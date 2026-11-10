use std::collections::HashMap;

use rsynapse_plugin::{
    Hint, LauncherActivity, Plugin, PluginGroup, PluginResponse, PluginView, QueryContext,
    ResultItem,
};
use zbus::{
    blocking::{Connection, Proxy},
    names::OwnedInterfaceName,
    zvariant::{OwnedObjectPath, OwnedValue},
};

const BUS: &str = "org.bluez";
const ROOT: &str = "/";
const DEVICE_INTERFACE: &str = "org.bluez.Device1";
const GROUP_ID: &str = "bluetooth";

type Interfaces = HashMap<OwnedInterfaceName, HashMap<String, OwnedValue>>;
type ManagedObjects = HashMap<OwnedObjectPath, Interfaces>;

#[derive(Clone, Debug, Eq, PartialEq)]
struct Device {
    path: String,
    name: String,
    address: String,
    icon: String,
    connected: bool,
    connecting: bool,
}

impl Device {
    fn subtitle(&self) -> String {
        if self.connected {
            "Connected".to_owned()
        } else if self.connecting {
            "Connecting...".to_owned()
        } else {
            self.address.clone()
        }
    }
}

pub struct BluetoothPlugin;

impl Plugin for BluetoothPlugin {
    fn name(&self) -> &'static str {
        "Bluetooth"
    }

    fn groups(&self) -> Vec<PluginGroup> {
        vec![PluginGroup {
            id: GROUP_ID.to_owned(),
            title: "Bluetooth".to_owned(),
            description: "Connect or disconnect Bluetooth devices".to_owned(),
            aliases: vec!["bluetooth".to_owned(), "bt".to_owned()],
            plugin: String::new(),
        }]
    }

    fn query(&self, context: &QueryContext<'_>) -> PluginResponse {
        if context.group != Some(GROUP_ID) {
            return PluginResponse::default();
        }

        let devices = match devices() {
            Ok(devices) => devices,
            Err(error) => {
                eprintln!("[bluetooth-plugin] {error}");
                return PluginResponse::default();
            }
        };
        let connected = devices.iter().filter(|device| device.connected).count();
        let subtitle = if connected == 0 {
            "No devices connected".to_owned()
        } else {
            format!("{connected} connected")
        };

        let query = context.query.trim().to_lowercase();
        let words = query.split_whitespace().collect::<Vec<_>>();
        let results = devices
            .into_iter()
            .filter(|device| {
                let text = format!("{} {}", device.name, device.address).to_lowercase();
                words.iter().all(|word| text.contains(word))
            })
            .enumerate()
            .map(|(index, device)| {
                let subtitle = device.subtitle();
                ResultItem {
                    id: format!("bluetooth:{}", device.path),
                    title: device.name,
                    description: subtitle,
                    icon: device.icon,
                    data: device.path,
                    score: 1_000.0 - index as f64,
                    current: device.connected,
                    keep_open: true,
                    activity: if device.connecting || !device.connected {
                        LauncherActivity::Connecting
                    } else {
                        LauncherActivity::None
                    },
                }
            })
            .collect();

        PluginResponse {
            view: Some(PluginView {
                title: "Bluetooth".to_owned(),
                subtitle,
                placeholder: "Search Bluetooth devices".to_owned(),
                hints: vec![
                    Hint {
                        key: "↑↓".to_owned(),
                        label: "choose".to_owned(),
                    },
                    Hint {
                        key: "Enter".to_owned(),
                        label: "toggle".to_owned(),
                    },
                    Hint {
                        key: "Esc".to_owned(),
                        label: "close".to_owned(),
                    },
                ],
            }),
            results,
        }
    }

    fn execute(&self, item: &ResultItem) -> Option<Result<(), String>> {
        let path = item.id.strip_prefix("bluetooth:")?;
        Some(toggle_device(path, item.current))
    }
}

fn devices() -> Result<Vec<Device>, String> {
    let connection = Connection::system().map_err(|error| format!("system bus: {error}"))?;
    let proxy = Proxy::new(&connection, BUS, ROOT, "org.freedesktop.DBus.ObjectManager")
        .map_err(|error| format!("BlueZ object manager: {error}"))?;
    let objects: ManagedObjects = proxy
        .call("GetManagedObjects", &())
        .map_err(|error| format!("GetManagedObjects: {error}"))?;

    let mut devices = objects
        .iter()
        .filter_map(|(path, interfaces)| device(path, interfaces))
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

fn device(path: &OwnedObjectPath, interfaces: &Interfaces) -> Option<Device> {
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

    Some(Device {
        path: path.as_str().to_owned(),
        name,
        address,
        icon,
        connected,
        connecting: bool_property(props, "Connecting").unwrap_or(false),
    })
}

fn symbolic_icon(bluez_icon: Option<&str>, connected: bool) -> String {
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

fn string_property(props: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    props.get(key)?.try_clone().ok()?.try_into().ok()
}

fn bool_property(props: &HashMap<String, OwnedValue>, key: &str) -> Option<bool> {
    props.get(key)?.try_clone().ok()?.try_into().ok()
}

fn toggle_device(path: &str, connected: bool) -> Result<(), String> {
    let connection = Connection::system().map_err(|error| format!("system bus: {error}"))?;
    let proxy = Proxy::new(&connection, BUS, path, DEVICE_INTERFACE)
        .map_err(|error| format!("BlueZ device {path}: {error}"))?;
    let method = if connected { "Disconnect" } else { "Connect" };
    let _: () = proxy
        .call(method, &())
        .map_err(|error| format!("{method} {path}: {error}"))?;
    Ok(())
}

pub fn plugin() -> Box<dyn Plugin> {
    Box::new(BluetoothPlugin)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str, connected: bool) -> Device {
        Device {
            path: format!("/org/bluez/hci0/dev_{name}"),
            name: name.to_owned(),
            address: "00:00:00:00:00:00".to_owned(),
            icon: "bluetooth".to_owned(),
            connected,
            connecting: false,
        }
    }

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

    #[test]
    fn connected_devices_sort_first() {
        let mut devices = vec![device("zeta", false), device("alpha", true)];
        devices.sort_by(|left, right| {
            right
                .connected
                .cmp(&left.connected)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
                .then_with(|| left.path.cmp(&right.path))
        });
        assert_eq!(devices[0].name, "alpha");
        assert_eq!(devices[1].name, "zeta");
    }

    #[test]
    fn connecting_device_reports_spinner_activity() {
        let mut device = device("headset", false);
        device.connecting = true;
        assert_eq!(device.subtitle(), "Connecting...");
        assert_eq!(
            if device.connecting {
                LauncherActivity::Connecting
            } else {
                LauncherActivity::None
            },
            LauncherActivity::Connecting
        );
    }
}
