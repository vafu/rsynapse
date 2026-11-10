use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::Notify;
use zbus::interface;

struct Source {
    flood: Arc<AtomicBool>,
    injected: Arc<AtomicBool>,
    read: Arc<Notify>,
    connection: Arc<std::sync::RwLock<Option<Connection>>>,
}

#[interface(name = "org.rsynapse.Workspace.Manager1")]
impl Source {
    #[zbus(property)]
    async fn connected(&self) -> bool {
        if self.flood.swap(false, Ordering::SeqCst) {
            let connection = self.connection.read().unwrap().as_ref().unwrap().clone();
            for _ in 0..512 {
                connection
                    .emit_signal(
                        None::<&str>,
                        "/org/rsynapse/Workspace/Windows/noisy",
                        "org.freedesktop.DBus.Properties",
                        "PropertiesChanged",
                        &(
                            workspace_dbus_api::WINDOW_INTERFACE,
                            std::collections::HashMap::from([(
                                "Title",
                                zbus::zvariant::Value::from("hotplug"),
                            )]),
                            Vec::<String>::new(),
                        ),
                    )
                    .await
                    .unwrap();
            }
            self.injected.store(true, Ordering::SeqCst);
        }
        true
    }

    #[zbus(property)]
    fn generation(&self) -> u64 {
        1
    }

    #[zbus(property)]
    fn windows(&self) -> Vec<OwnedObjectPath> {
        vec![]
    }

    #[zbus(property)]
    fn workspaces(&self) -> Vec<OwnedObjectPath> {
        if self.injected.load(Ordering::SeqCst) {
            self.read.notify_one();
        }
        vec![]
    }
}

struct Relations;
#[interface(name = "org.rsynapse.Locus.Relations1")]
impl Relations {
    fn list_with_persistence(&self, _relation: &str) -> Vec<locus::RelationState> {
        vec![]
    }
    fn list(&self, _relation: &str) -> Vec<locus::RelationRecord> {
        vec![]
    }
}

#[test]
fn hotplug_signal_burst_cannot_hold_the_binding_lock() {
    if std::env::var_os("WORKSPACE_BACKPRESSURE_WORKER").is_none() {
        assert!(std::process::Command::new("dbus-run-session").arg("--")
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", "associations::workspace_cleanup::backpressure::hotplug_signal_burst_cannot_hold_the_binding_lock", "--nocapture"])
            .env("WORKSPACE_BACKPRESSURE_WORKER", "1").status().unwrap().success());
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let flood = Arc::new(AtomicBool::new(false));
        let injected = Arc::new(AtomicBool::new(false));
        let read = Arc::new(Notify::new());
        let connection = Arc::new(std::sync::RwLock::new(None));
        let _source = zbus::connection::Builder::session()
            .unwrap()
            .serve_at(ROOT_PATH, zbus::fdo::ObjectManager)
            .unwrap()
            .serve_at(
                ROOT_PATH,
                Source {
                    flood: flood.clone(),
                    injected,
                    read: read.clone(),
                    connection: connection.clone(),
                },
            )
            .unwrap()
            .name(BUS_NAME)
            .unwrap()
            .build()
            .await
            .unwrap();
        *connection.write().unwrap() = Some(_source.clone());
        let _locus = zbus::connection::Builder::session()
            .unwrap()
            .serve_at(locus::OBJECT_PATH, Relations)
            .unwrap()
            .name(locus::BUS_NAME)
            .unwrap()
            .build()
            .await
            .unwrap();
        let locus = LocusClient::connect().await.unwrap();
        flood.store(true, Ordering::SeqCst);
        let cleanup = tokio::spawn(run(locus));
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            read.notified().await;
            let _guard = super::super::BIND_LOCK.lock().await;
        })
        .await
        .expect("hotplug signal backpressure blocked cleanup and project assignment");
        assert!(
            !cleanup.is_finished(),
            "cleanup task must remain live after the burst"
        );
        cleanup.abort();
    });
}
