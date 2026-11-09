mod component;
mod model;
mod source;
mod view;

pub(super) use component::BluetoothIndicator;

use shell_core::source::Observable;
use zbus::zvariant::OwnedObjectPath;

use crate::widgets::nerd_icon::NerdIcon;
use crate::widgets::nerd_icon::md;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct BluetoothView {
    pub(super) status: BluetoothStatusView,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct BluetoothStatusView {
    pub(super) icon: NerdIcon,
    pub(super) connected_count: u8,
    pub(super) powered: bool,
    pub(super) adapter_path: Option<OwnedObjectPath>,
}

impl Default for BluetoothStatusView {
    fn default() -> Self {
        Self {
            icon: NerdIcon::new(md::MD_BLUETOOTH_OFF),
            connected_count: 0,
            powered: false,
            adapter_path: None,
        }
    }
}

pub(super) fn bluetooth_status() -> Observable<BluetoothView> {
    source::bluetooth_status()
}

pub(super) fn toggle_power(status: &BluetoothStatusView) {
    source::set_adapter_power(status.adapter_path.clone(), !status.powered);
}

pub(super) fn status_count(status: &BluetoothStatusView) -> String {
    if status.connected_count == 0 {
        String::new()
    } else {
        status.connected_count.to_string()
    }
}

pub(super) fn status_tooltip(status: &BluetoothStatusView) -> String {
    if !status.powered {
        "Bluetooth disabled".to_owned()
    } else if status.connected_count == 0 {
        "Bluetooth".to_owned()
    } else {
        format!("{} Bluetooth device(s) connected", status.connected_count)
    }
}
