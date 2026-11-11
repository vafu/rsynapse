use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
};

use futures_util::StreamExt;
use shell_rx_macros::combine_latest;
use zbus::{
    Connection, Proxy,
    zvariant::{OwnedObjectPath, OwnedValue},
};

use super::{DEFAULT_ITEM_PATH, ITEM_INTERFACE, WATCHER_BUS, WATCHER_INTERFACE, WATCHER_PATH};
use crate::{
    Observable,
    dbus::{self, Bus, ObjectDescriptor, PropertyDescriptor, service_owner},
    from_task, once,
    rx::Observable as _,
    shared_by_key, switch_map, write,
};

type RawPixmap = (i32, i32, Vec<u8>);
type RawTooltip = (String, Vec<RawPixmap>, String, String);

/// Stable address of one registered StatusNotifierItem.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct TrayItemKey {
    service: String,
    path: String,
}

impl TrayItemKey {
    /// Parses a StatusNotifierWatcher registration string.
    ///
    /// Registrations are either a bus name (using the default item path) or a
    /// bus name followed by an object path.
    pub fn parse(registration: &str) -> Option<Self> {
        let registration = registration.trim();
        if registration.is_empty() {
            return None;
        }

        let (service, path) = match registration.split_once('/') {
            Some((service, path)) if !service.is_empty() => {
                (service.to_owned(), format!("/{path}"))
            }
            Some(_) => return None,
            None => (registration.to_owned(), DEFAULT_ITEM_PATH.to_owned()),
        };

        if !valid_service_name(&service) || !path.starts_with('/') {
            return None;
        }

        Some(Self { service, path })
    }

    pub fn service(&self) -> &str {
        &self.service
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub(super) fn stable_key(&self) -> String {
        format!("{}{}", self.service, self.path)
    }
}

fn object(service: &str, path: &str) -> ObjectDescriptor {
    ObjectDescriptor::parse(Bus::Session, service, path, ITEM_INTERFACE)
        .expect("StatusNotifierItem descriptor should be valid")
}

/// Current StatusNotifierItem status.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TrayStatus {
    #[default]
    Passive,
    Active,
    NeedsAttention,
}

impl From<&str> for TrayStatus {
    fn from(status: &str) -> Self {
        match status.trim() {
            "Active" => Self::Active,
            "NeedsAttention" => Self::NeedsAttention,
            _ => Self::Passive,
        }
    }
}

/// Raw ARGB32 tray icon bytes in network byte order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrayPixmap {
    pub width: u32,
    pub height: u32,
    pub argb: Vec<u8>,
}

/// Icon supplied by a tray item.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum TrayIcon {
    #[default]
    None,
    Name(String),
    Pixmap(Vec<TrayPixmap>),
}

/// Tooltip text supplied by a tray item.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TrayTooltip {
    pub title: String,
    pub description: String,
}

/// Live snapshot of one StatusNotifierItem.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TrayItem {
    pub key: TrayItemKey,
    pub id: String,
    pub title: String,
    pub status: TrayStatus,
    pub icon: TrayIcon,
    pub overlay_icon: TrayIcon,
    pub attention_icon: TrayIcon,
    pub tooltip: TrayTooltip,
    pub item_is_menu: bool,
    pub menu_path: Option<String>,
    pub icon_theme_path: Option<String>,
}

impl TrayItem {
    fn with_key(key: TrayItemKey) -> Self {
        Self {
            key,
            ..Self::default()
        }
    }

    pub fn has_menu(&self) -> bool {
        self.menu_path.is_some()
    }

    pub fn display_title(&self) -> &str {
        let title = self.title.trim();
        if title.is_empty() {
            self.id.trim()
        } else {
            title
        }
    }

    pub fn active_icon(&self) -> &TrayIcon {
        if self.status == TrayStatus::NeedsAttention
            && !matches!(self.attention_icon, TrayIcon::None)
        {
            &self.attention_icon
        } else {
            &self.icon
        }
    }

    pub fn tooltip_text(&self) -> String {
        let title = self.tooltip.title.trim();
        let description = self.tooltip.description.trim();
        if !title.is_empty() && !description.is_empty() {
            format!("{title}\n{description}")
        } else if !title.is_empty() {
            title.to_owned()
        } else if !description.is_empty() {
            description.to_owned()
        } else {
            self.display_title().to_owned()
        }
    }
}

/// Emits the registered tray item addresses in watcher order.
pub fn item_keys() -> Observable<Vec<TrayItemKey>> {
    shared_by_key("systray-item-keys", "session", || {
        switch_map(
            service_owner(Bus::Session, WATCHER_BUS),
            |owner| match owner {
                Some(owner) => watcher_registrations(owner),
                None => once(Vec::new()),
            },
        )
        .distinct_until_changed()
        .box_it()
    })
}

/// Emits the live snapshot for one tray item.
pub fn item(key: TrayItemKey) -> Observable<TrayItem> {
    shared_by_key("systray-item", key.stable_key(), move || {
        item_snapshot(key.clone()).distinct_until_changed().box_it()
    })
}

/// Requests an item's primary action.
pub fn activate(key: &TrayItemKey) {
    call_item(key.clone(), "Activate", "systray activate");
}

/// Requests an item's secondary action.
pub fn secondary_activate(key: &TrayItemKey) {
    call_item(
        key.clone(),
        "SecondaryActivate",
        "systray secondary activate",
    );
}

/// Requests an item's context menu at the provided position.
pub fn context_menu(key: &TrayItemKey) {
    call_item(key.clone(), "ContextMenu", "systray context menu");
}

/// Sends a scroll event to an item.
pub fn scroll(key: &TrayItemKey, delta: i32, orientation: &'static str) {
    let key = key.clone();
    write::spawn("systray scroll", async move {
        let proxy = item_proxy(&key).await?;
        proxy
            .call::<_, _, ()>("Scroll", &(delta, orientation))
            .await
            .map_err(|error| error.to_string())
    });
}

fn call_item(key: TrayItemKey, method: &'static str, label: &'static str) {
    write::spawn(label, async move {
        let proxy = item_proxy(&key).await?;
        proxy
            .call::<_, _, ()>(method, &(0i32, 0i32))
            .await
            .map_err(|error| error.to_string())
    });
}

async fn item_proxy(key: &TrayItemKey) -> Result<Proxy<'static>, String> {
    let connection = write::session_connection()
        .await
        .map_err(|error| error.to_string())?;
    Proxy::new(
        connection,
        key.service.clone(),
        key.path.clone(),
        ITEM_INTERFACE,
    )
    .await
    .map_err(|error| error.to_string())
}

fn watcher_registrations(owner: String) -> Observable<Vec<TrayItemKey>> {
    from_task(move |sender| {
        let owner = owner.clone();
        async move {
            let result: Result<(), String> = async {
                let connection = Connection::session()
                    .await
                    .map_err(|error| format!("connect tray session bus failed: {error}"))?;
                let proxy =
                    Proxy::new(&connection, owner.as_str(), WATCHER_PATH, WATCHER_INTERFACE)
                        .await
                        .map_err(|error| format!("connect tray watcher failed: {error}"))?;
                let mut registered = proxy
                    .receive_signal("StatusNotifierItemRegistered")
                    .await
                    .map_err(|error| format!("watch tray registrations failed: {error}"))?;
                let mut unregistered = proxy
                    .receive_signal("StatusNotifierItemUnregistered")
                    .await
                    .map_err(|error| format!("watch tray removals failed: {error}"))?;
                let mut properties_changed = proxy
                    .receive_signal("PropertiesChanged")
                    .await
                    .map_err(|error| format!("watch tray watcher properties failed: {error}"))?;

                let mut registrations: Vec<String> = proxy
                    .get_property("RegisteredStatusNotifierItems")
                    .await
                    .map_err(|error| format!("read registered tray items failed: {error}"))?;
                let mut keys = parse_keys(&registrations);
                if sender.send(Ok(keys.clone())).await.is_err() {
                    return Ok(());
                }

                loop {
                    tokio::select! {
                        item = registered.next() => {
                            let Some(message) = item else {
                                return Ok(());
                            };
                            if let Ok(registration) =
                                message.body().deserialize::<String>()
                            {
                                push_unique(&mut registrations, registration);
                            }
                        }
                        item = unregistered.next() => {
                            let Some(message) = item else {
                                return Ok(());
                            };
                            if let Ok(registration) =
                                message.body().deserialize::<String>()
                            {
                                registrations.retain(|item| item != &registration);
                            }
                        }
                        item = properties_changed.next() => {
                            let Some(message) = item else {
                                return Ok(());
                            };
                            let Ok((interface, changed, _invalidated)) = message
                                .body()
                                .deserialize::<(
                                    String,
                                    HashMap<String, OwnedValue>,
                                    Vec<String>,
                                )>()
                            else {
                                continue;
                            };
                            if interface != WATCHER_INTERFACE {
                                continue;
                            }
                            let Some(value) =
                                changed.get("RegisteredStatusNotifierItems")
                            else {
                                continue;
                            };
                            let Ok(items) =
                                value.try_clone().and_then(Vec::<String>::try_from)
                            else {
                                continue;
                            };
                            registrations = items;
                        }
                    }

                    let next = parse_keys(&registrations);
                    if next != keys {
                        keys = next;
                        if sender.send(Ok(keys.clone())).await.is_err() {
                            return Ok(());
                        }
                    }
                }
            }
            .await;

            if let Err(error) = result {
                let _ = sender.send(Err(error)).await;
            }
        }
    })
    .distinct_until_changed()
    .box_it()
}

fn parse_keys(registrations: &[String]) -> Vec<TrayItemKey> {
    let mut seen = HashSet::with_capacity(registrations.len());
    let mut keys = Vec::with_capacity(registrations.len());
    for registration in registrations {
        let Some(key) = TrayItemKey::parse(registration) else {
            continue;
        };
        if seen.insert(key.clone()) {
            keys.push(key);
        }
    }
    keys
}

fn push_unique(registrations: &mut Vec<String>, registration: String) {
    if !registrations.iter().any(|item| item == &registration) {
        registrations.push(registration);
    }
}

fn valid_service_name(service: &str) -> bool {
    service.starts_with(':') || service.contains('.')
}

fn item_snapshot(key: TrayItemKey) -> Observable<TrayItem> {
    let key_for_owner = key.clone();
    switch_map(
        service_owner(Bus::Session, key.service.clone()),
        move |owner| match owner {
            Some(owner) => item_snapshot_for(key_for_owner.clone(), owner),
            None => once(TrayItem::with_key(key_for_owner.clone())),
        },
    )
    .distinct_until_changed()
    .box_it()
}

fn item_snapshot_for(key: TrayItemKey, owner: String) -> Observable<TrayItem> {
    let object = object(&owner, key.path());
    let identity = combine_latest!(
        property(&object, "Id", String::new()),
        property(&object, "Title", String::new()),
        property(&object, "Status", "Passive".to_owned())
            .map(|status| TrayStatus::from(status.as_str()))
            .distinct_until_changed()
            .box_it(),
    )
    .box_it();
    let icons = combine_latest!(
        property(&object, "IconName", String::new()),
        property::<Vec<RawPixmap>>(&object, "IconPixmap", Vec::new()),
        property(&object, "OverlayIconName", String::new()),
        property::<Vec<RawPixmap>>(&object, "OverlayIconPixmap", Vec::new()),
        property(&object, "AttentionIconName", String::new()),
        property::<Vec<RawPixmap>>(&object, "AttentionIconPixmap", Vec::new())
            => |(icon_name, pixmaps, overlay_name, overlay_pixmaps, attention_name, attention_pixmaps)| IconSet {
                icon: tray_icon(icon_name, pixmaps),
                overlay: tray_icon(overlay_name, overlay_pixmaps),
                attention: tray_icon(attention_name, attention_pixmaps),
            },
    )
    .distinct_until_changed()
    .box_it();
    let behavior = combine_latest!(
        property(&object, "ItemIsMenu", false),
        menu_path(&key, &owner),
        property(&object, "IconThemePath", String::new())
            .map(non_blank)
            .distinct_until_changed()
            .box_it(),
    )
    .box_it();
    let default_tooltip: RawTooltip = (String::new(), Vec::new(), String::new(), String::new());
    let tooltip = property(&object, "ToolTip", default_tooltip)
        .map(|(_icon_name, _pixmaps, title, description)| TrayTooltip { title, description })
        .distinct_until_changed()
        .box_it();
    let key_for_view = key.clone();

    combine_latest!(identity, icons, behavior, tooltip => move |(identity, icons, behavior, tooltip)| {
        let (id, title, status) = identity;
        let (item_is_menu, menu_path, icon_theme_path) = behavior;
        TrayItem {
            key: key_for_view.clone(),
            id,
            title,
            status,
            icon: icons.icon,
            overlay_icon: icons.overlay,
            attention_icon: icons.attention,
            tooltip,
            item_is_menu,
            menu_path,
            icon_theme_path,
        }
    })
    .distinct_until_changed()
    .box_it()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct IconSet {
    icon: TrayIcon,
    overlay: TrayIcon,
    attention: TrayIcon,
}

fn non_blank(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn tray_icon(name: String, pixmaps: Vec<RawPixmap>) -> TrayIcon {
    let name = name.trim();
    if !name.is_empty() {
        TrayIcon::Name(name.to_owned())
    } else {
        let pixmaps = pixmaps
            .into_iter()
            .filter_map(|(width, height, argb)| {
                let width = u32::try_from(width).ok()?;
                let height = u32::try_from(height).ok()?;
                (width > 0 && height > 0 && argb.len() >= (width * height * 4) as usize).then_some(
                    TrayPixmap {
                        width,
                        height,
                        argb,
                    },
                )
            })
            .collect::<Vec<_>>();
        if pixmaps.is_empty() {
            TrayIcon::None
        } else {
            TrayIcon::Pixmap(pixmaps)
        }
    }
}

fn property<T>(object: &ObjectDescriptor, name: &'static str, default: T) -> Observable<T>
where
    T: TryFrom<OwnedValue> + Clone + PartialEq + Send + 'static,
    T::Error: Display,
{
    // StatusNotifierItem fields are optional and third-party items (e.g. Slack)
    // may reject reads while still being present. Treat a failed read as absent
    // instead of a source error.
    dbus::property_or_lenient(PropertyDescriptor::new(object.clone(), name), default)
}

pub(super) fn menu_path(key: &TrayItemKey, owner: &str) -> Observable<Option<String>> {
    let object = object(owner, key.path());
    dbus::property::<OwnedObjectPath>(PropertyDescriptor::new(object, "Menu"))
        .map(|path| {
            path.and_then(|path| {
                let path = path.as_str().trim();
                (!path.is_empty() && path != "/NO_DBUSMENU").then(|| path.to_owned())
            })
        })
        .distinct_until_changed()
        .box_it()
}

#[cfg(test)]
mod tests {
    use super::{TrayItemKey, TrayStatus};

    #[test]
    fn parses_service_and_path_registrations() {
        let key = TrayItemKey::parse(":1.42/org/ayatana/NotificationItem/foo").unwrap();
        assert_eq!(key.service(), ":1.42");
        assert_eq!(key.path(), "/org/ayatana/NotificationItem/foo");
    }

    #[test]
    fn parses_service_only_registrations_with_default_path() {
        let key = TrayItemKey::parse("org.kde.StatusNotifierItem-1234-1").unwrap();
        assert_eq!(key.service(), "org.kde.StatusNotifierItem-1234-1");
        assert_eq!(key.path(), "/StatusNotifierItem");
    }

    #[test]
    fn rejects_path_only_and_invalid_registrations() {
        assert!(TrayItemKey::parse("/org/foo").is_none());
        assert!(TrayItemKey::parse("").is_none());
        assert!(TrayItemKey::parse("no-dots").is_none());
    }

    #[test]
    fn parses_status_values() {
        assert_eq!(TrayStatus::from("Active"), TrayStatus::Active);
        assert_eq!(
            TrayStatus::from("NeedsAttention"),
            TrayStatus::NeedsAttention
        );
        assert_eq!(TrayStatus::from("Passive"), TrayStatus::Passive);
        assert_eq!(TrayStatus::from("other"), TrayStatus::Passive);
    }
}
