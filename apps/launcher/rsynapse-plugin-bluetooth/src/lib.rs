use data_sources::bluetooth;
use rsynapse_plugin::{
    Hint, LauncherActivity, Plugin, PluginGroup, PluginResponse, PluginView, QueryContext,
    ResultItem,
};

const GROUP_ID: &str = "bluetooth";

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

        let devices = match bluetooth::devices_blocking() {
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
                let icon = device.symbolic_icon();
                ResultItem {
                    id: format!("bluetooth:{}", device.path),
                    title: device.name,
                    description: subtitle,
                    icon,
                    data: device.path,
                    score: 1_000.0 - index as f64,
                    current: device.connected,
                    keep_open: true,
                    activity: if device.connecting {
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
        Some(bluetooth::toggle_device_blocking(path, item.current))
    }
}

pub fn plugin() -> Box<dyn Plugin> {
    Box::new(BluetoothPlugin)
}
