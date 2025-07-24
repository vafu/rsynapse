use anyhow::Result;
use libloading::{Library, Symbol};
use rsynapse_plugin::ResultItem as PluginResultItem;
use rsynapse_plugin::{Plugin, ResultItem};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use zbus::{ConnectionBuilder, interface, zvariant::Type}; // Rename for clarity

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

    unsafe fn load_from(&mut self, path: &str) -> Result<()> {
        for entry in std::fs::read_dir(path)? {
            let path = entry?.path();
            if path.is_file() && path.extension().map_or(false, |e| e == "so") {
                let lib = unsafe { Library::new(&path) }?;
                let constructor: Symbol<unsafe extern "C" fn() -> *mut dyn Plugin> =
                    unsafe { lib.get(b"_rsynapse_init")? };
                let plugin = unsafe { Box::from_raw(constructor()) };
                println!("[Daemon] Loaded plugin: {}", plugin.name());
                self.plugins.push(plugin);
                self._libraries.push(lib);
            }
        }
        Ok(())
    }
}

struct Launcher {
    manager: Arc<PluginManager>,
}

#[derive(Debug, Clone, Type, Serialize, Deserialize)]
struct DbusResultItem {
    id: String,
    title: String,
    description: String,
    icon: String,
    command: String,
}

// Implement a conversion from the plugin's struct to our D-Bus struct.
// This keeps the conversion logic clean and reusable.
impl From<PluginResultItem> for DbusResultItem {
    fn from(item: PluginResultItem) -> Self {
        Self {
            id: item.id,
            title: item.title,
            description: item.description.unwrap_or_default(),
            icon: item.icon.unwrap_or_default(),
            command: item.command.unwrap_or_default(),
        }
    }
}

#[interface(name = "org.rsynapse.Engine1")]
impl Launcher {
    async fn search(&self, query: &str) -> Vec<DbusResultItem> {
        let mut all_results: Vec<ResultItem> = Vec::new();

        for plugin in &self.manager.plugins {
            all_results.extend(plugin.query(query));
        }

        // Convert rich `ResultItem` to a simple string for the CLI.
        all_results.into_iter().map(DbusResultItem::from).collect()
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
        manager: Arc::new(manager),
    };

    let _conn = ConnectionBuilder::session()?
        .name("com.rsynapse.Engine")?
        .serve_at("/org/rsynapse/Engine1", launcher)?
        .build()
        .await?;

    println!("[Daemon] rsynapse server is running.");
    std::future::pending::<()>().await;

    Ok(())
}
