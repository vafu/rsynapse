use std::collections::HashMap;

use futures_util::StreamExt;
use zbus::{
    Connection, Proxy,
    zvariant::{OwnedValue, Value},
};

use super::{MENU_INTERFACE, TrayIcon, TrayItemKey, TrayPixmap};
use crate::{
    Observable,
    dbus::{Bus, service_owner},
    from_task, once,
    rx::Observable as _,
    shared_by_key, switch_map, write,
};

type RawPixmap = (i32, i32, Vec<u8>);

/// Snapshot of one DBusMenu root.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TrayMenu {
    pub item: TrayItemKey,
    pub path: String,
    pub revision: u32,
    pub items: Vec<TrayMenuItem>,
}

/// Kind of one DBusMenu row.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TrayMenuKind {
    #[default]
    Standard,
    Separator,
    Submenu,
}

/// Toggle state of one DBusMenu row.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TrayToggle {
    #[default]
    None,
    Check(bool),
    Radio(bool),
}

/// One DBusMenu row.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TrayMenuItem {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub visible: bool,
    pub kind: TrayMenuKind,
    pub toggle: TrayToggle,
    pub icon: TrayIcon,
    pub children: Vec<TrayMenuItem>,
}

impl TrayMenuItem {
    pub fn is_visible(&self) -> bool {
        self.visible
    }
}

impl TrayMenu {
    fn empty(item: TrayItemKey) -> Self {
        Self {
            item,
            ..Self::default()
        }
    }
}

/// Emits the live DBusMenu tree for a tray item.
pub fn menu(key: TrayItemKey) -> Observable<TrayMenu> {
    let cache_key = key.stable_key();
    shared_by_key("systray-menu", cache_key, move || {
        let key = key.clone();
        switch_map(
            service_owner(Bus::Session, key.service().to_owned()),
            move |owner| match owner {
                Some(owner) => menu_for_owner(key.clone(), owner),
                None => once(TrayMenu::empty(key.clone())),
            },
        )
        .distinct_until_changed()
        .box_it()
    })
}

fn menu_for_owner(key: TrayItemKey, owner: String) -> Observable<TrayMenu> {
    let key_for_path = key.clone();
    switch_map(
        super::item::menu_path(&key_for_path, &owner),
        move |path| match path {
            Some(path) => menu_tree(key.clone(), owner.clone(), path),
            None => once(TrayMenu::empty(key.clone())),
        },
    )
    .box_it()
}

/// Sends a DBusMenu event for one row.
pub fn menu_event(menu: &TrayMenu, id: i32, event: &str) {
    let key = menu.item.clone();
    let path = menu.path.clone();
    let event = event.to_owned();
    write::spawn("systray menu event", async move {
        if path.is_empty() {
            return Ok::<(), String>(());
        }
        let connection = write::session_connection()
            .await
            .map_err(|error| error.to_string())?;
        let proxy = Proxy::new(connection, key.service(), path.as_str(), MENU_INTERFACE)
            .await
            .map_err(|error| error.to_string())?;
        let data = OwnedValue::try_from(Value::from("")).map_err(|error| error.to_string())?;
        proxy
            .call::<_, _, ()>("Event", &(id, event, data, 0u32))
            .await
            .map_err(|error| error.to_string())
    });
}

/// Asks a DBusMenu row to populate its children before they are shown.
pub fn menu_about_to_show(menu: &TrayMenu, id: i32) {
    let key = menu.item.clone();
    let path = menu.path.clone();
    write::spawn("systray menu about-to-show", async move {
        if path.is_empty() {
            return Ok::<(), String>(());
        }
        let connection = write::session_connection()
            .await
            .map_err(|error| error.to_string())?;
        let proxy = Proxy::new(connection, key.service(), path.as_str(), MENU_INTERFACE)
            .await
            .map_err(|error| error.to_string())?;
        proxy
            .call::<_, _, bool>("AboutToShow", &(id,))
            .await
            .map_err(|error| error.to_string())?;
        Ok::<(), String>(())
    });
}

fn menu_tree(key: TrayItemKey, owner: String, path: String) -> Observable<TrayMenu> {
    from_task(move |sender| {
        let key = key.clone();
        let owner = owner.clone();
        let path = path.clone();
        async move {
            let result: Result<(), String> = async {
                let connection = Connection::session()
                    .await
                    .map_err(|error| format!("connect tray menu bus failed: {error}"))?;
                let proxy = Proxy::new(&connection, owner.as_str(), path.as_str(), MENU_INTERFACE)
                    .await
                    .map_err(|error| format!("connect tray menu {path} failed: {error}"))?;
                let mut layout_updated = proxy
                    .receive_signal("LayoutUpdated")
                    .await
                    .map_err(|error| format!("watch tray menu layout failed: {error}"))?;
                let mut items_updated = proxy
                    .receive_signal("ItemsPropertiesUpdated")
                    .await
                    .map_err(|error| format!("watch tray menu items failed: {error}"))?;

                let _ = proxy.call::<_, _, bool>("AboutToShow", &(0i32,)).await;
                let mut current = fetch_menu(&proxy, &key, &path).await?;
                if sender.send(Ok(current.clone())).await.is_err() {
                    return Ok(());
                }

                loop {
                    tokio::select! {
                        item = layout_updated.next() => {
                            if item.is_none() {
                                return Ok(());
                            }
                        }
                        item = items_updated.next() => {
                            if item.is_none() {
                                return Ok(());
                            }
                        }
                    }

                    let next = fetch_menu(&proxy, &key, &path).await?;
                    if next != current {
                        current = next;
                        if sender.send(Ok(current.clone())).await.is_err() {
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

async fn fetch_menu(proxy: &Proxy<'_>, key: &TrayItemKey, path: &str) -> Result<TrayMenu, String> {
    let (revision, root): (u32, RawMenuNode) = proxy
        .call("GetLayout", &(0i32, -1i32, Vec::<String>::new()))
        .await
        .map_err(|error| format!("read tray menu {path} failed: {error}"))?;

    Ok(TrayMenu {
        item: key.clone(),
        path: path.to_owned(),
        revision,
        items: root
            .children
            .into_iter()
            .filter_map(|child| decode::<RawMenuNode>(&child))
            .map(parse_node)
            .collect(),
    })
}

#[derive(
    Debug,
    serde::Deserialize,
    serde::Serialize,
    zbus::zvariant::Type,
    zbus::zvariant::OwnedValue,
    zbus::zvariant::Value,
)]
struct RawMenuNode {
    id: i32,
    properties: HashMap<String, OwnedValue>,
    children: Vec<OwnedValue>,
}

fn parse_node(raw: RawMenuNode) -> TrayMenuItem {
    let RawMenuNode {
        id,
        properties,
        children,
    } = raw;
    let label = string_prop(&properties, "label").unwrap_or_default();
    let enabled = bool_prop(&properties, "enabled").unwrap_or(true);
    let visible = bool_prop(&properties, "visible").unwrap_or(true);
    let node_type = string_prop(&properties, "type").unwrap_or_default();
    let children_display = string_prop(&properties, "children-display").unwrap_or_default();
    let toggle = match string_prop(&properties, "toggle-type").as_deref() {
        Some("checkmark") => TrayToggle::Check(i32_prop(&properties, "toggle-state") == Some(1)),
        Some("radio") => TrayToggle::Radio(i32_prop(&properties, "toggle-state") == Some(1)),
        _ => TrayToggle::None,
    };
    let children = children
        .into_iter()
        .filter_map(|child| decode::<RawMenuNode>(&child))
        .map(parse_node)
        .collect::<Vec<_>>();
    let kind = if node_type == "separator" {
        TrayMenuKind::Separator
    } else if children_display == "submenu" || !children.is_empty() {
        TrayMenuKind::Submenu
    } else {
        TrayMenuKind::Standard
    };

    TrayMenuItem {
        id,
        label,
        enabled,
        visible,
        kind,
        toggle,
        icon: menu_icon(&properties),
        children,
    }
}

fn menu_icon(properties: &HashMap<String, OwnedValue>) -> TrayIcon {
    if let Some(name) = string_prop(properties, "icon-name") {
        let name = name.trim();
        if !name.is_empty() {
            return TrayIcon::Name(name.to_owned());
        }
    }

    let Some(value) = properties.get("icon-data") else {
        return TrayIcon::None;
    };
    let pixmaps = decode::<(i32, i32, Vec<u8>)>(value)
        .map(|pixmap| vec![pixmap])
        .or_else(|| decode::<Vec<RawPixmap>>(value))
        .unwrap_or_default();
    let pixmaps = pixmaps
        .into_iter()
        .filter_map(valid_pixmap)
        .collect::<Vec<_>>();
    if pixmaps.is_empty() {
        TrayIcon::None
    } else {
        TrayIcon::Pixmap(pixmaps)
    }
}

fn valid_pixmap((width, height, argb): RawPixmap) -> Option<TrayPixmap> {
    let width = u32::try_from(width).ok()?;
    let height = u32::try_from(height).ok()?;
    (width > 0 && height > 0 && argb.len() >= (width * height * 4) as usize).then_some(TrayPixmap {
        width,
        height,
        argb,
    })
}

fn string_prop(properties: &HashMap<String, OwnedValue>, name: &str) -> Option<String> {
    decode(properties.get(name)?)
}

fn bool_prop(properties: &HashMap<String, OwnedValue>, name: &str) -> Option<bool> {
    decode(properties.get(name)?)
}

fn i32_prop(properties: &HashMap<String, OwnedValue>, name: &str) -> Option<i32> {
    decode(properties.get(name)?)
}

fn decode<T>(value: &OwnedValue) -> Option<T>
where
    T: TryFrom<OwnedValue>,
{
    let mut value = value.try_clone().ok()?;
    for _ in 0..4 {
        if let Ok(decoded) = T::try_from(value.try_clone().ok()?) {
            return Some(decoded);
        }
        let next = match &*value {
            Value::Value(inner) => inner.try_clone().ok()?,
            _ => return None,
        };
        value = OwnedValue::try_from(next).ok()?;
    }
    None
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use zbus::zvariant::OwnedValue;

    use super::{RawMenuNode, TrayMenu, TrayMenuKind, TrayToggle, decode, parse_node};
    use crate::systray::TrayItemKey;

    fn ov(value: impl Into<OwnedValue>) -> OwnedValue {
        value.into()
    }

    fn ov_str(value: &str) -> OwnedValue {
        OwnedValue::try_from(zbus::zvariant::Value::from(value.to_owned())).unwrap()
    }

    fn props(entries: &[(&str, OwnedValue)]) -> HashMap<String, OwnedValue> {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.try_clone().unwrap()))
            .collect()
    }

    #[test]
    fn parses_standard_and_separator_rows() {
        let item = parse_node(RawMenuNode {
            id: 3,
            properties: props(&[("label", ov_str("Open"))]),
            children: Vec::new(),
        });
        assert_eq!(item.id, 3);
        assert_eq!(item.label, "Open");
        assert_eq!(item.kind, TrayMenuKind::Standard);
        assert!(item.enabled);
        assert!(item.visible);

        let separator = parse_node(RawMenuNode {
            id: 4,
            properties: props(&[("type", ov_str("separator"))]),
            children: Vec::new(),
        });
        assert_eq!(separator.kind, TrayMenuKind::Separator);
    }

    #[test]
    fn parses_submenu_and_checkmark() {
        let child = OwnedValue::try_from(zbus::zvariant::Value::from(RawMenuNode {
            id: 6,
            properties: props(&[("label", ov_str("Child"))]),
            children: Vec::new(),
        }))
        .unwrap();
        let item = parse_node(RawMenuNode {
            id: 5,
            properties: props(&[
                ("label", ov_str("More")),
                ("children-display", ov_str("submenu")),
                ("toggle-type", ov_str("checkmark")),
                ("toggle-state", ov(1i32)),
            ]),
            children: vec![child],
        });

        assert_eq!(item.kind, TrayMenuKind::Submenu);
        assert_eq!(item.toggle, TrayToggle::Check(true));
        assert_eq!(item.children.len(), 1);
        assert_eq!(item.children[0].label, "Child");
    }

    #[test]
    fn raw_menu_node_has_dbusmenu_layout_signature() {
        use zbus::zvariant::Type as _;

        assert_eq!(RawMenuNode::signature().as_str(), "(ia{sv}av)");
    }

    #[test]
    fn raw_menu_node_roundtrips_through_dbus_encoding() {
        let child = OwnedValue::try_from(zbus::zvariant::Value::from(RawMenuNode {
            id: 8,
            properties: props(&[("label", ov_str("Child"))]),
            children: Vec::new(),
        }))
        .unwrap();
        let node = RawMenuNode {
            id: 7,
            properties: props(&[("label", ov_str("Open"))]),
            children: vec![child],
        };
        let ctxt = zbus::zvariant::serialized::Context::new_dbus(zbus::zvariant::LE, 0);
        let encoded = zbus::zvariant::to_bytes(ctxt, &node).unwrap();
        let (decoded, _): (RawMenuNode, _) = encoded.deserialize().unwrap();
        assert_eq!(decoded.id, 7);
        assert_eq!(decoded.children.len(), 1);
        assert_eq!(decode::<RawMenuNode>(&decoded.children[0]).unwrap().id, 8);
        assert_eq!(
            decoded
                .properties
                .get("label")
                .and_then(|value| String::try_from(value.try_clone().unwrap()).ok())
                .as_deref(),
            Some("Open")
        );
    }

    #[test]
    fn empty_menu_keeps_item_key() {
        let key = TrayItemKey::parse(":1.5/StatusNotifierItem").unwrap();
        let menu = TrayMenu::empty(key.clone());
        assert_eq!(menu.item, key);
        assert!(menu.items.is_empty());
    }
}
