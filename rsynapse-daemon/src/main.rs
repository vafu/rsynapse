use anyhow::{Context, Result};
use libloading::{Library, Symbol};
use notify::{RecursiveMode, Watcher};
use rsynapse_plugin::{Plugin, ResultItem};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex, RwLock};
use zbus::{ConnectionBuilder, interface, zvariant::Type};

// --- Config ---

#[derive(Deserialize, Default)]
struct DaemonConfig {
    #[serde(default)]
    plugins: HashMap<String, PluginConfig>,
}

#[derive(Deserialize, Default)]
struct PluginConfig {
    execute: Option<String>,
}

fn load_config() -> DaemonConfig {
    let path = match dirs::config_dir() {
        Some(dir) => dir.join("rsynapse/config.toml"),
        None => return DaemonConfig::default(),
    };

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[Daemon] No config at {:?}: {}", path, e);
            return DaemonConfig::default();
        }
    };

    match toml::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[Daemon] Failed to parse config: {}", e);
            DaemonConfig::default()
        }
    }
}

// --- Plugin Manager ---

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

// --- D-Bus types ---

#[derive(Debug, Clone, Type, Serialize, Deserialize)]
struct DbusResultItem {
    id: String,
    title: String,
    description: String,
    icon: String,
    data: String,
}

// --- Cached result for Execute ---

struct CachedResult {
    plugin_name: String,
    item: DbusResultItem,
}

// --- D-Bus interface ---

struct Engine {
    manager: Arc<PluginManager>,
    config: Arc<RwLock<DaemonConfig>>,
    execute_defaults: Arc<HashMap<String, String>>,
    last_results: Arc<Mutex<Vec<CachedResult>>>,
}

#[interface(name = "org.rsynapse.Engine1")]
impl Engine {
    async fn search(&self, query: &str) -> Vec<DbusResultItem> {
        let mut tagged: Vec<(f64, String, DbusResultItem)> = Vec::new();

        for plugin in &self.manager.plugins {
            let name = plugin.name().to_string();
            for item in plugin.query(query) {
                let score = item.score;
                tagged.push((score, name.clone(), DbusResultItem::from(item)));
            }
        }

        tagged.sort_by(|a, b| {
            b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut cache = self.last_results.lock().unwrap();
        cache.clear();

        let results: Vec<DbusResultItem> = tagged
            .into_iter()
            .map(|(_, plugin_name, item)| {
                cache.push(CachedResult {
                    plugin_name,
                    item: item.clone(),
                });
                item
            })
            .collect();

        results
    }

    async fn execute(&self, id: &str) -> String {
        let cache = self.last_results.lock().unwrap();

        let cached = match cache.iter().find(|r| r.item.id == id) {
            Some(r) => r,
            None => {
                let msg = format!("No result with id '{}'", id);
                eprintln!("[Daemon] {}", msg);
                return format!("Error: {}", msg);
            }
        };

        let config = self.config.read().unwrap();
        let config_execute = config.plugins.get(&cached.plugin_name)
            .and_then(|cfg| cfg.execute.as_deref());

        let template = if let Some(tmpl) = config_execute {
            tmpl
        } else if let Some(default) = self.execute_defaults.get(&cached.plugin_name) {
            default.as_str()
        } else {
            let msg = format!("No execute config for plugin '{}'", cached.plugin_name);
            eprintln!("[Daemon] {}", msg);
            return format!("Error: {}", msg);
        };

        let command = template
            .replace("{id}", &cached.item.id)
            .replace("{title}", &cached.item.title)
            .replace("{description}", &cached.item.description)
            .replace("{icon}", &cached.item.icon)
            .replace("{data}", &cached.item.data);

        eprintln!("[Daemon] Executing: {}", command);

        match Command::new("sh").arg("-c").arg(&command).spawn() {
            Ok(_) => String::new(),
            Err(e) => {
                let msg = format!("Failed to execute: {}", e);
                eprintln!("[Daemon] {}", msg);
                format!("Error: {}", msg)
            }
        }
    }
}

impl From<ResultItem> for DbusResultItem {
    fn from(item: ResultItem) -> Self {
        Self {
            id: item.id,
            title: item.title,
            description: item.description.unwrap_or_default(),
            icon: item.icon.unwrap_or_default(),
            data: item.data.unwrap_or_default(),
        }
    }
}

// --- Main ---

fn get_plugin_path() -> Option<PathBuf> {
    if cfg!(debug_assertions) {
        println!("[Daemon] Using DEBUG plugin path.");
        Some(PathBuf::from("./target/debug/"))
    } else {
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
    let config = load_config();

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

    let mut execute_defaults = HashMap::new();
    for plugin in &manager.plugins {
        if let Some(default) = plugin.default_execute() {
            execute_defaults.insert(plugin.name().to_string(), default.to_string());
        }
    }

    let manager = Arc::new(manager);
    let config = Arc::new(RwLock::new(config));

    let engine = Engine {
        manager: Arc::clone(&manager),
        config: Arc::clone(&config),
        execute_defaults: Arc::new(execute_defaults),
        last_results: Arc::new(Mutex::new(Vec::new())),
    };

    // Watch config file for changes.
    if let Some(config_dir) = dirs::config_dir() {
        let config_path = config_dir.join("rsynapse");
        let manager_ref = Arc::clone(&manager);
        let config_ref = Arc::clone(&config);

        std::thread::spawn(move || {
            let (tx, rx) = std::sync::mpsc::channel();
            let mut watcher = notify::recommended_watcher(tx).unwrap();

            if config_path.exists() {
                watcher.watch(&config_path, RecursiveMode::NonRecursive).unwrap();
                eprintln!("[Daemon] Watching config at {:?}", config_path);
            }

            for res in rx {
                if let Ok(_event) = res {
                    eprintln!("[Daemon] Config changed, reloading...");
                    *config_ref.write().unwrap() = load_config();
                    for plugin in &manager_ref.plugins {
                        plugin.reload();
                    }
                }
            }
        });
    }

    let _conn = ConnectionBuilder::session()?
        .name("com.rsynapse.Engine")?
        .serve_at("/org/rsynapse/Engine1", engine)?
        .build()
        .await?;

    println!("[Daemon] rsynapse server is running.");
    std::future::pending::<()>().await;

    Ok(())
}
