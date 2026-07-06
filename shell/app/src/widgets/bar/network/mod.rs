mod model;

use shell_core::source::{self, Observable, rx::Observable as _};

use self::model::{NetworkDeviceSnapshot, NetworkKind, network_devices};

const DEVICE_STATE_UNAVAILABLE: u32 = 20;
const DEVICE_STATE_DISCONNECTED: u32 = 30;
const DEVICE_STATE_PREPARE: u32 = 40;
const DEVICE_STATE_SECONDARIES: u32 = 90;
const DEVICE_STATE_ACTIVATED: u32 = 100;
const DEVICE_STATE_DEACTIVATING: u32 = 110;
const DEVICE_STATE_FAILED: u32 = 120;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct NetworkView {
    pub(super) wifi: WifiView,
    pub(super) ethernet: EthernetView,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WifiView {
    pub(super) visible: bool,
    pub(super) icon: String,
    pub(super) tooltip: String,
}

impl Default for WifiView {
    fn default() -> Self {
        Self {
            visible: false,
            icon: "network-wireless-offline-symbolic".to_owned(),
            tooltip: String::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct EthernetView {
    pub(super) visible: bool,
    pub(super) icon: String,
    pub(super) tooltip: String,
}

impl Default for EthernetView {
    fn default() -> Self {
        Self {
            visible: false,
            icon: "network-wired-disconnected-symbolic".to_owned(),
            tooltip: String::new(),
        }
    }
}

pub(super) fn network_status() -> Observable<NetworkView> {
    source::shared_by_key("rsynapse.network-status", "all", || {
        network_devices()
            .map(network_view)
            .distinct_until_changed()
            .box_it()
    })
}

fn network_view(devices: Vec<Option<NetworkDeviceSnapshot>>) -> NetworkView {
    let devices = devices.into_iter().flatten().collect::<Vec<_>>();
    let wifi = devices
        .iter()
        .filter(|device| device.kind == NetworkKind::Wifi)
        .min_by_key(|device| device_rank(device.state))
        .map(wifi_view)
        .unwrap_or_default();
    let ethernet = devices
        .iter()
        .filter(|device| device.kind == NetworkKind::Ethernet)
        .min_by_key(|device| device_rank(device.state))
        .map(ethernet_view)
        .unwrap_or_default();

    NetworkView { wifi, ethernet }
}

fn wifi_view(device: &NetworkDeviceSnapshot) -> WifiView {
    let strength = device
        .access_point
        .as_ref()
        .map(|access_point| access_point.strength)
        .unwrap_or(0);
    let ssid = device
        .access_point
        .as_ref()
        .and_then(|access_point| access_point.ssid.as_deref());

    WifiView {
        visible: true,
        icon: wifi_icon(device.state, strength).to_owned(),
        tooltip: match (device.state, ssid) {
            (DEVICE_STATE_ACTIVATED, Some(ssid)) if strength > 0 => {
                format!("Wi-Fi: {ssid} ({strength}%)")
            }
            (DEVICE_STATE_ACTIVATED, Some(ssid)) => format!("Wi-Fi: {ssid}"),
            _ => format_device_tooltip("Wi-Fi", device),
        },
    }
}

fn ethernet_view(device: &NetworkDeviceSnapshot) -> EthernetView {
    EthernetView {
        visible: true,
        icon: ethernet_icon(device.state).to_owned(),
        tooltip: format_device_tooltip("Ethernet", device),
    }
}

fn wifi_icon(state: u32, strength: u8) -> &'static str {
    if state == DEVICE_STATE_ACTIVATED {
        match strength {
            80..=100 => "network-wireless-signal-excellent-symbolic",
            60..=79 => "network-wireless-signal-good-symbolic",
            40..=59 => "network-wireless-signal-ok-symbolic",
            1..=39 => "network-wireless-signal-weak-symbolic",
            _ => "network-wireless-signal-none-symbolic",
        }
    } else if is_connecting(state) {
        "network-wireless-acquiring-symbolic"
    } else if state == DEVICE_STATE_FAILED {
        "network-wireless-disabled-symbolic"
    } else {
        "network-wireless-offline-symbolic"
    }
}

fn ethernet_icon(state: u32) -> &'static str {
    if state == DEVICE_STATE_ACTIVATED {
        "network-wired-symbolic"
    } else if is_connecting(state) {
        "network-wired-acquiring-symbolic"
    } else if state == DEVICE_STATE_FAILED {
        "network-error-symbolic"
    } else {
        "network-wired-disconnected-symbolic"
    }
}

fn format_device_tooltip(label: &str, device: &NetworkDeviceSnapshot) -> String {
    let state = state_label(device.state);
    if device.interface.is_empty() {
        format!("{label}: {state}")
    } else {
        format!("{label}: {} ({state})", device.interface)
    }
}

fn state_label(state: u32) -> &'static str {
    match state {
        DEVICE_STATE_UNAVAILABLE => "unavailable",
        DEVICE_STATE_DISCONNECTED => "disconnected",
        DEVICE_STATE_ACTIVATED => "connected",
        DEVICE_STATE_DEACTIVATING => "disconnecting",
        DEVICE_STATE_FAILED => "failed",
        state if is_connecting(state) => "connecting",
        _ => "unknown",
    }
}

fn device_rank(state: u32) -> u8 {
    match state {
        DEVICE_STATE_ACTIVATED => 0,
        state if is_connecting(state) => 1,
        DEVICE_STATE_FAILED => 2,
        DEVICE_STATE_DISCONNECTED | DEVICE_STATE_UNAVAILABLE => 3,
        _ => 4,
    }
}

fn is_connecting(state: u32) -> bool {
    (DEVICE_STATE_PREPARE..=DEVICE_STATE_SECONDARIES).contains(&state)
}

#[cfg(test)]
mod tests {
    use super::{ethernet_icon, wifi_icon};

    #[test]
    fn wifi_icon_tracks_strength_when_connected() {
        assert_eq!(
            wifi_icon(100, 84),
            "network-wireless-signal-excellent-symbolic"
        );
        assert_eq!(wifi_icon(100, 24), "network-wireless-signal-weak-symbolic");
        assert_eq!(wifi_icon(50, 84), "network-wireless-acquiring-symbolic");
    }

    #[test]
    fn ethernet_icon_tracks_connection_state() {
        assert_eq!(ethernet_icon(100), "network-wired-symbolic");
        assert_eq!(ethernet_icon(40), "network-wired-acquiring-symbolic");
        assert_eq!(ethernet_icon(30), "network-wired-disconnected-symbolic");
    }
}
