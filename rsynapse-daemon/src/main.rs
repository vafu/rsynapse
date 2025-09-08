use anyhow::{Context, Result};
use libloading::{Library, Symbol};
use rsynapse_plugin::ResultItem as PluginResultItem;
use rsynapse_plugin::{Plugin, ResultItem};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
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

    unsafe fn load_plugins_from(&mut self, path: &PathBuf) -> Result<()> {
        println!("[Daemon] Loading plugins from: {:?}", path);

        for entry in std::fs::read_dir(path)
            .with_context(|| format!("Failed to read plugin directory at: {:?}", path))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |e| e == "so") {
                println!("[Daemon] Attempting to load library: {:?}", path);

                let lib = unsafe { Library::new(&path) }?;
                let constructor: Symbol<unsafe extern "C" fn() -> *mut dyn Plugin> =
                    unsafe { lib.get(b"_rsynapse_init") }?;
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

        all_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Convert rich `ResultItem` to a simple string for the CLI.
        all_results.into_iter().map(DbusResultItem::from).collect()
    }
}

fn get_plugin_path() -> Option<PathBuf> {
    if cfg!(debug_assertions) {
        // In debug builds, use the local target directory.
        println!("[Daemon] Using DEBUG plugin path.");
        Some(PathBuf::from("./target/debug/"))
    } else {
        // In release builds, use the installed location in the home directory.
        println!("[Daemon] Using RELEASE plugin path.");
        dirs::home_dir().map(|mut path| {
            path.push(".local/lib/rsynapse/plugins/");
            path
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut manager = PluginManager::new();

    eprintln!("[Daemon] [INFO] Daemon starting up. Determining plugin path...");
    if let Some(plugin_path) = get_plugin_path() {
        if plugin_path.exists() {
            unsafe {
                manager.load_plugins_from(&plugin_path)?;
            }
        } else {
            eprintln!(
                "[Daemon] Warning: Plugin directory does not exist at {:?}",
                plugin_path
            );
        }
    } else {
        eprintln!("[Daemon] Error: Could not determine plugin path.");
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
