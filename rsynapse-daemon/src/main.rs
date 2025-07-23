// ./rsynapse-daemon/src/main.rs
use anyhow::Result;
use libloading::{Library, Symbol};
use rsynapse_plugin::{Plugin, ResultItem};
use std::sync::{Arc, Mutex};
use zbus::{dbus_interface, interface, ConnectionBuilder};

struct PluginManager {
    plugins: Vec<Box<dyn Plugin>>,
    _libraries: Vec<Library>,
}

impl PluginManager {
    fn new() -> Self {
        Self {
            plugins: Vec::new(),
            _libraries: Vec::new(),
        }
    }

    /// Loads plugins from a given directory.
    ///
    /// # Safety
    /// This function is unsafe because it loads and executes foreign code
    /// from dynamic libraries. The libraries must be trusted and adhere
    /// to the `_rsynapse_init` function signature and Rust's ABI.
    unsafe fn load_from(&mut self, path: &str) -> Result<()> {
        for entry in std::fs::read_dir(path)? {
            let path = entry?.path();
            if path.is_file() && path.extension().map_or(false, |e| e == "so") {
                let lib = Library::new(&path)?;
                let constructor: Symbol<unsafe extern "C" fn() -> *mut dyn Plugin> =
                    lib.get(b"_rsynapse_init")?;
                let plugin = Box::from_raw(constructor());
                println!("[Daemon] Loaded plugin: {}", plugin.name());
                self.plugins.push(plugin);
                self._libraries.push(lib);
            }
        }
        Ok(())
    }
}

struct Launcher {
    manager: Arc<Mutex<PluginManager>>,
}

#[interface(name = "org.rsynapse.Launcher1")]
impl Launcher {
    async fn search(&self, query: &str) -> Vec<String> {
        let manager = self.manager.lock().unwrap();
        let mut results: Vec<ResultItem> = Vec::new();

        for plugin in &manager.plugins {
            results.extend(plugin.query(query));
        }

        // Convert rich `ResultItem` to a simple string for the CLI.
        results.into_iter().map(|item| item.title).collect()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut manager = PluginManager::new();

    // In a real application, this should be a dedicated, secure plugin directory.
    // For now, we load from the build output directory.
    unsafe {
        manager.load_from("./target/debug/")?;
    }

    if manager.plugins.is_empty() {
        eprintln!("[Daemon] Warning: No plugins loaded. The daemon will not return any results.");
    }

    let launcher = Launcher {
        manager: Arc::new(Mutex::new(manager)),
    };

    let _conn = ConnectionBuilder::session()?
        .name("com.rsynapse.Launcher")?
        .serve_at("/org/rsynapse/Launcher1", launcher)?
        .build()
        .await?;

    println!("[Daemon] rsynapse server is running.");
    std::future::pending::<()>().await;

    Ok(())
}
