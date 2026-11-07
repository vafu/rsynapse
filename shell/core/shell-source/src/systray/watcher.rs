use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use futures_util::StreamExt;
use zbus::{
    Connection, fdo, interface, message::Header, object_server::SignalContext, zvariant::Value,
};

use super::{DEFAULT_ITEM_PATH, WATCHER_BUS, WATCHER_INTERFACE, WATCHER_PATH};
use crate::write;

#[derive(Default)]
struct WatcherState {
    items: Vec<String>,
}

struct Watcher {
    state: Arc<Mutex<WatcherState>>,
}

#[interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    async fn register_status_notifier_item(
        &self,
        service: String,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(signal_context)] context: SignalContext<'_>,
    ) -> fdo::Result<()> {
        let Some(registration) = registration(&service, header.sender()) else {
            return Err(fdo::Error::InvalidArgs(
                "invalid StatusNotifierItem service or object path".into(),
            ));
        };

        let items = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| fdo::Error::Failed("tray watcher state lock poisoned".into()))?;
            if state.items.contains(&registration) {
                None
            } else {
                state.items.push(registration.clone());
                Some(state.items.clone())
            }
        };
        let Some(items) = items else {
            return Ok(());
        };

        Self::status_notifier_item_registered(&context, &registration)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        emit_items_changed(connection, &items)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Ok(())
    }

    async fn register_status_notifier_host(&self, _service: String) -> fdo::Result<()> {
        Ok(())
    }

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.state
            .lock()
            .map(|state| state.items.clone())
            .unwrap_or_default()
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }

    #[zbus(signal)]
    async fn status_notifier_item_registered(
        context: &SignalContext<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_item_unregistered(
        context: &SignalContext<'_>,
        service: &str,
    ) -> zbus::Result<()>;
}

/// Starts the shell's StatusNotifierWatcher host once per process.
///
/// If another watcher already owns the bus name, the existing host is used and
/// this process does not replace it.
pub fn start_watcher_host() {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }

    write::spawn("systray watcher host", async move {
        run_watcher_host().await.map_err(|error| error.to_string())
    });
}

async fn run_watcher_host() -> zbus::Result<()> {
    let connection = Connection::session().await?;
    let state = Arc::new(Mutex::new(WatcherState::default()));
    connection
        .object_server()
        .at(
            WATCHER_PATH,
            Watcher {
                state: state.clone(),
            },
        )
        .await?;

    let reply = connection
        .request_name_with_flags(
            WATCHER_BUS,
            fdo::RequestNameFlags::AllowReplacement | fdo::RequestNameFlags::DoNotQueue,
        )
        .await?;
    if !matches!(
        reply,
        fdo::RequestNameReply::PrimaryOwner | fdo::RequestNameReply::AlreadyOwner
    ) {
        return Ok(());
    }

    tokio::spawn(cleanup_disconnected_items(connection.clone(), state));
    std::future::pending::<()>().await;
    Ok(())
}

async fn cleanup_disconnected_items(connection: Connection, state: Arc<Mutex<WatcherState>>) {
    let Ok(bus) = fdo::DBusProxy::new(&connection).await else {
        return;
    };
    let Ok(mut changes) = bus.receive_name_owner_changed().await else {
        return;
    };
    let Ok(context) = SignalContext::new(&connection, WATCHER_PATH) else {
        return;
    };

    while let Some(event) = changes.next().await {
        let Ok(args) = event.args() else {
            continue;
        };
        if args.new_owner().is_some() {
            continue;
        }
        let name = args.name().as_str().to_owned();
        let (removed, items) = {
            let Ok(mut state) = state.lock() else {
                return;
            };
            let mut removed = Vec::new();
            state.items.retain(|registration| {
                if registration_service(registration) == name {
                    removed.push(registration.clone());
                    false
                } else {
                    true
                }
            });
            (removed, state.items.clone())
        };
        if removed.is_empty() {
            continue;
        }
        for registration in &removed {
            let _ = Watcher::status_notifier_item_unregistered(&context, registration).await;
        }
        let _ = emit_items_changed(&connection, &items).await;
    }
}

async fn emit_items_changed(connection: &Connection, items: &[String]) -> zbus::Result<()> {
    let mut changed = HashMap::new();
    changed.insert("RegisteredStatusNotifierItems", Value::from(items.to_vec()));
    connection
        .emit_signal(
            None::<&str>,
            WATCHER_PATH,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &(WATCHER_INTERFACE.to_owned(), changed, Vec::<String>::new()),
        )
        .await
}

fn registration(service: &str, sender: Option<&zbus::names::UniqueName<'_>>) -> Option<String> {
    let service = service.trim();
    if service.is_empty() {
        return None;
    }

    let (service, path) = if service.starts_with('/') {
        (sender?.as_str().to_owned(), service.to_owned())
    } else if let Some((service, path)) = service.split_once('/') {
        (service.to_owned(), format!("/{path}"))
    } else {
        (service.to_owned(), DEFAULT_ITEM_PATH.to_owned())
    };

    if service.is_empty() || !path.starts_with('/') {
        return None;
    }
    Some(format!("{service}{path}"))
}

fn registration_service(registration: &str) -> &str {
    registration
        .split_once('/')
        .map(|(service, _)| service)
        .unwrap_or(registration)
}

#[cfg(test)]
mod tests {
    use zbus::names::UniqueName;

    use super::registration;

    #[test]
    fn registration_defaults_to_item_path() {
        assert_eq!(
            registration("org.kde.StatusNotifierItem-1", None).as_deref(),
            Some("org.kde.StatusNotifierItem-1/StatusNotifierItem")
        );
    }

    #[test]
    fn registration_preserves_explicit_path() {
        assert_eq!(
            registration("org.kde.StatusNotifierItem-1/MyItem", None).as_deref(),
            Some("org.kde.StatusNotifierItem-1/MyItem")
        );
    }

    #[test]
    fn registration_uses_sender_for_object_paths() {
        let sender = UniqueName::try_from(":1.42").unwrap();
        assert_eq!(
            registration("/MyItem", Some(&sender)).as_deref(),
            Some(":1.42/MyItem")
        );
    }

    #[test]
    fn registration_rejects_path_without_sender() {
        assert!(registration("/MyItem", None).is_none());
    }
}
