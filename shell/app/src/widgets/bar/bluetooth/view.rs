use zbus::zvariant::OwnedObjectPath;

use super::{BluetoothStatusView, BluetoothView};
use crate::widgets::nerd_icon::NerdIcon;
use crate::widgets::nerd_icon::md;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AdapterSnapshot {
    path: OwnedObjectPath,
    powered: bool,
}

pub(super) fn adapter_snapshot(path: OwnedObjectPath, powered: bool) -> AdapterSnapshot {
    AdapterSnapshot { path, powered }
}

pub(super) fn bluetooth_view(
    adapters: Vec<AdapterSnapshot>,
    connected: Vec<bool>,
) -> BluetoothView {
    let adapter = adapters.into_iter().next();
    let powered = adapter
        .as_ref()
        .map(|adapter| adapter.powered)
        .unwrap_or(false);
    let connected_count = connected
        .into_iter()
        .filter(|connected| *connected)
        .count()
        .min(u8::MAX as usize) as u8;

    BluetoothView {
        status: BluetoothStatusView {
            icon: status_icon(powered, connected_count),
            connected_count,
            powered,
            adapter_path: adapter.map(|adapter| adapter.path),
        },
    }
}

fn status_icon(powered: bool, connected_count: u8) -> NerdIcon {
    if !powered {
        NerdIcon::new(md::MD_BLUETOOTH_OFF)
    } else if connected_count > 0 {
        NerdIcon::new(md::MD_BLUETOOTH_CONNECT)
    } else {
        NerdIcon::new(md::MD_BLUETOOTH)
    }
}

#[cfg(test)]
mod tests {
    use crate::widgets::nerd_icon::md;

    use super::status_icon;

    #[test]
    fn status_icon_tracks_power_and_connections() {
        assert_eq!(status_icon(false, 0).glyph(), md::MD_BLUETOOTH_OFF);
        assert_eq!(status_icon(true, 0).glyph(), md::MD_BLUETOOTH);
        assert_eq!(status_icon(true, 2).glyph(), md::MD_BLUETOOTH_CONNECT);
    }
}
