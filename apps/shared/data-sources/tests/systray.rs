use std::{
    collections::HashMap,
    process::Command,
    sync::{Arc, Mutex},
    time::Duration,
};

use data_sources::{rx::Observable as _, systray};
use futures_util::StreamExt;
use zbus::{
    interface,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

const WATCHER_BUS: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const ITEM_BUS: &str = "org.rsynapse.TestTrayItem";
const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU_PATH: &str = "/Menu";

type RawTooltip = (String, Vec<(i32, i32, Vec<u8>)>, String, String);

struct Watcher {
    items: Arc<Mutex<Vec<String>>>,
}

#[interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items.lock().unwrap().clone()
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }
}

struct Item;

#[interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    #[zbus(property)]
    fn id(&self) -> &str {
        "test"
    }

    #[zbus(property)]
    fn title(&self) -> &str {
        "Test Tray"
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "NeedsAttention"
    }

    #[zbus(property)]
    fn icon_name(&self) -> &str {
        "application-x-executable"
    }

    #[zbus(property)]
    fn icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        Vec::new()
    }

    #[zbus(property)]
    fn overlay_icon_name(&self) -> &str {
        ""
    }

    #[zbus(property)]
    fn overlay_icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        Vec::new()
    }

    #[zbus(property)]
    fn attention_icon_name(&self) -> &str {
        ""
    }

    #[zbus(property)]
    fn attention_icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        Vec::new()
    }

    #[zbus(property)]
    fn tool_tip(&self) -> RawTooltip {
        (
            String::new(),
            Vec::new(),
            "Tooltip title".into(),
            "Tooltip body".into(),
        )
    }

    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn menu(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(MENU_PATH).unwrap()
    }
}

struct Menu;

#[interface(name = "com.canonical.dbusmenu")]
impl Menu {
    async fn get_layout(
        &self,
        _parent_id: i32,
        _recursion_depth: i32,
        _property_names: Vec<String>,
    ) -> (u32, (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>)) {
        let mut properties = properties(&[
            ("label", "Open"),
            ("enabled", "true"),
            ("visible", "true"),
            ("toggle-type", "checkmark"),
        ]);
        properties.insert("toggle-state".into(), OwnedValue::from(1i32));
        let child = OwnedValue::try_from(Value::from(TestNode {
            id: 1,
            properties,
            children: Vec::new(),
        }))
        .unwrap();
        (3, (0, HashMap::new(), vec![child]))
    }

    async fn about_to_show(&self, _id: i32) -> bool {
        true
    }

    async fn event(&self, _id: i32, _event_id: String, _data: Value<'_>, _timestamp: u32) {}

    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }

    #[zbus(property)]
    fn text_direction(&self) -> &str {
        "ltr"
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "normal"
    }
}

#[derive(
    Debug, serde::Serialize, zbus::zvariant::Type, zbus::zvariant::OwnedValue, zbus::zvariant::Value,
)]
struct TestNode {
    id: i32,
    properties: HashMap<String, OwnedValue>,
    children: Vec<OwnedValue>,
}

fn properties(entries: &[(&str, &str)]) -> HashMap<String, OwnedValue> {
    entries
        .iter()
        .map(|(key, value)| {
            (
                (*key).to_owned(),
                OwnedValue::try_from(Value::from((*value).to_owned())).unwrap(),
            )
        })
        .collect()
}

#[test]
#[ignore = "requires dbus-run-session"]
fn tray_items_and_menu_follow_status_notifier_and_dbusmenu() {
    if std::env::var_os("SYSTRAY_SOURCE_CHILD").is_none() {
        let status = Command::new("dbus-run-session")
            .arg("--")
            .arg(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "tray_items_and_menu_follow_status_notifier_and_dbusmenu",
                "--nocapture",
            ])
            .env("SYSTRAY_SOURCE_CHILD", "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let watcher_items = Arc::new(Mutex::new(vec![format!("{ITEM_BUS}{ITEM_PATH}")]));
            let watcher = Watcher {
                items: watcher_items.clone(),
            };
            let connection = zbus::connection::Builder::session()
                .unwrap()
                .serve_at(WATCHER_PATH, watcher)
                .unwrap()
                .serve_at(ITEM_PATH, Item)
                .unwrap()
                .serve_at(MENU_PATH, Menu)
                .unwrap()
                .build()
                .await
                .unwrap();
            connection.request_name(WATCHER_BUS).await.unwrap();
            connection.request_name(ITEM_BUS).await.unwrap();

            let mut keys = systray::item_keys().into_stream();
            let key = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let keys = keys.next().await.unwrap().unwrap();
                    if let Some(key) = keys.into_iter().next() {
                        return key;
                    }
                }
            })
            .await
            .expect("tray item registration");

            let mut items = systray::item(key.clone()).into_stream();
            let item = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let item = items.next().await.unwrap().unwrap();
                    if item.title == "Test Tray" {
                        return item;
                    }
                }
            })
            .await
            .expect("tray item snapshot");
            assert_eq!(key.service(), ITEM_BUS);
            assert_eq!(item.status, systray::TrayStatus::NeedsAttention);
            assert_eq!(item.tooltip.title, "Tooltip title");
            assert_eq!(item.tooltip.description, "Tooltip body");
            assert_eq!(item.menu_path.as_deref(), Some(MENU_PATH));

            let mut menus = systray::menu(key).into_stream();
            let menu = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let menu = menus.next().await.unwrap().unwrap();
                    if !menu.items.is_empty() {
                        return menu;
                    }
                }
            })
            .await
            .expect("tray menu");
            assert_eq!(menu.revision, 3);
            assert_eq!(menu.items.len(), 1);
            assert_eq!(menu.items[0].label, "Open");
            assert_eq!(menu.items[0].toggle, systray::TrayToggle::Check(true));
        });
}

#[test]
#[ignore = "requires dbus-run-session"]
fn tray_watcher_host_accepts_registrations() {
    if std::env::var_os("SYSTRAY_HOST_CHILD").is_none() {
        let status = Command::new("dbus-run-session")
            .arg("--")
            .arg(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "tray_watcher_host_accepts_registrations",
                "--nocapture",
            ])
            .env("SYSTRAY_HOST_CHILD", "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let connection = zbus::connection::Builder::session()
                .unwrap()
                .serve_at(ITEM_PATH, Item)
                .unwrap()
                .serve_at(MENU_PATH, Menu)
                .unwrap()
                .build()
                .await
                .unwrap();
            connection.request_name(ITEM_BUS).await.unwrap();

            systray::start_watcher_host();

            let bus = zbus::fdo::DBusProxy::new(&connection).await.unwrap();
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let name = zbus::names::BusName::try_from(WATCHER_BUS).unwrap();
                    if bus.get_name_owner(name).await.is_ok() {
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("tray watcher host");

            let watcher = zbus::Proxy::new(&connection, WATCHER_BUS, WATCHER_PATH, WATCHER_BUS)
                .await
                .unwrap();
            let _: () = watcher
                .call("RegisterStatusNotifierItem", &(ITEM_BUS,))
                .await
                .unwrap();

            let mut keys = systray::item_keys().into_stream();
            let key = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let keys = keys.next().await.unwrap().unwrap();
                    if let Some(key) = keys.into_iter().next() {
                        return key;
                    }
                }
            })
            .await
            .expect("registered tray item");
            assert_eq!(key.service(), ITEM_BUS);
            assert_eq!(key.path(), ITEM_PATH);
        });
}
