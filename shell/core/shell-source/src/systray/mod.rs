//! StatusNotifierItem tray discovery, item snapshots, and DBusMenu access.

mod item;
mod menu;
mod watcher;

pub use item::{
    TrayIcon, TrayItem, TrayItemKey, TrayPixmap, TrayStatus, TrayTooltip, activate, context_menu,
    item, item_keys, scroll, secondary_activate,
};
pub use menu::{
    TrayMenu, TrayMenuItem, TrayMenuKind, TrayToggle, menu, menu_about_to_show, menu_event,
};
pub use watcher::start_watcher_host;

use crate::{Observable, keyed_list, materialize_list, rx::Observable as _, shared_by_key};

pub(crate) const WATCHER_BUS: &str = "org.kde.StatusNotifierWatcher";
pub(crate) const WATCHER_PATH: &str = "/StatusNotifierWatcher";
pub(crate) const WATCHER_INTERFACE: &str = "org.kde.StatusNotifierWatcher";
pub(crate) const ITEM_INTERFACE: &str = "org.kde.StatusNotifierItem";
pub(crate) const MENU_INTERFACE: &str = "com.canonical.dbusmenu";
pub(crate) const DEFAULT_ITEM_PATH: &str = "/StatusNotifierItem";

/// Emits full snapshots for every registered StatusNotifierItem.
pub fn items() -> Observable<Vec<TrayItem>> {
    shared_by_key("systray-items", "session", || {
        materialize_list(|| keyed_list(item_keys, |key| key.clone(), item))
            .distinct_until_changed()
            .box_it()
    })
}
