use anyhow::{Context, Result};
use libloading::{Library, Symbol};
use notify::{
    EventKind, RecursiveMode, Watcher,
    event::{AccessKind, AccessMode, CreateKind},
};
use rsynapse_launcher_api::{
    DescribeResponse, KeyEvent as DbusKeyEvent, KeyRequest, KeyResponse, LauncherGroup,
    LauncherHint, LauncherResult, LauncherView, SearchRequest, SearchResponse,
};
use rsynapse_plugin::{Key, KeyEvent, KeyOutcome, Plugin, PluginGroup, QueryContext, ResultItem};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex, RwLock};
use zbus::{ConnectionBuilder, interface};

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

struct LoadedGroup {
    group: PluginGroup,
    plugin_index: usize,
}

struct PluginManager {
    plugins: Vec<Box<dyn Plugin>>,
    groups: Vec<LoadedGroup>,
    _libraries: Vec<Library>,
}

impl PluginManager {
    fn new() -> Self {
        Self {
            plugins: Vec::new(),
            groups: Vec::new(),
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

                let plugin_index = self.plugins.len();
                for group in plugin.groups() {
                    self.groups.push(LoadedGroup {
                        group,
                        plugin_index,
                    });
                }
                self.plugins.push(plugin);
                self._libraries.push(lib);
            }
        }
        Ok(())
    }

    fn resolve_group(&self, param: &str) -> Option<&LoadedGroup> {
        self.groups.iter().find(|loaded| {
            loaded.group.id.eq_ignore_ascii_case(param)
                || loaded
                    .group
                    .aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(param))
        })
    }

    fn group_infos(&self) -> Vec<LauncherGroup> {
        self.groups
            .iter()
            .map(|loaded| LauncherGroup {
                id: loaded.group.id.clone(),
                title: loaded.group.title.clone(),
                description: loaded.group.description.clone().unwrap_or_default(),
                aliases: loaded.group.aliases.clone(),
                plugin: self.plugins[loaded.plugin_index].name().to_owned(),
            })
            .collect()
    }

    fn view_for_group(&self, group_id: &str) -> LauncherView {
        match self
            .groups
            .iter()
            .find(|loaded| loaded.group.id == group_id)
        {
            Some(loaded) => view_for_group(&loaded.group),
            None => default_view(),
        }
    }
}

// --- Request resolution ---

struct ResolvedRequest<'a> {
    focused_group: Option<&'a str>,
    params: Vec<String>,
    plugin_indices: Vec<usize>,
}

fn resolve_request<'a>(manager: &'a PluginManager, request: &SearchRequest) -> ResolvedRequest<'a> {
    if let Some(first) = request.params.first() {
        if let Some(loaded) = manager.resolve_group(first) {
            return ResolvedRequest {
                focused_group: Some(loaded.group.id.as_str()),
                params: request.params[1..].to_vec(),
                plugin_indices: vec![loaded.plugin_index],
            };
        }
    }

    ResolvedRequest {
        focused_group: None,
        params: request.params.clone(),
        plugin_indices: (0..manager.plugins.len()).collect(),
    }
}

fn key_request_context<'a>(
    manager: &'a PluginManager,
    params: &'a [String],
) -> (Option<&'a str>, Vec<String>) {
    if let Some(first) = params.first() {
        if let Some(loaded) = manager.resolve_group(first) {
            return (Some(loaded.group.id.as_str()), params[1..].to_vec());
        }
    }
    (None, params.to_vec())
}

// --- D-Bus types ---

#[derive(Debug, Clone, Default)]
struct CachedSearch {
    results: Vec<CachedResult>,
}

#[derive(Debug, Clone)]
struct CachedResult {
    plugin_name: String,
    item: ResultItem,
}

fn launcher_result(item: ResultItem) -> LauncherResult {
    LauncherResult {
        id: item.id,
        title: item.title,
        description: item.description.unwrap_or_default(),
        icon: item.icon.unwrap_or_default(),
        data: item.data.unwrap_or_default(),
        current: item.current,
    }
}

fn default_hints() -> Vec<LauncherHint> {
    vec![
        LauncherHint {
            key: "↑↓".to_owned(),
            label: "choose".to_owned(),
        },
        LauncherHint {
            key: "Enter".to_owned(),
            label: "apply".to_owned(),
        },
        LauncherHint {
            key: "Esc".to_owned(),
            label: "close".to_owned(),
        },
    ]
}

fn view_for_group(group: &PluginGroup) -> LauncherView {
    LauncherView {
        title: group.title.clone(),
        subtitle: group.description.clone().unwrap_or_default(),
        placeholder: format!("Search {}", group.title.to_lowercase()),
        hints: default_hints(),
    }
}

fn default_view() -> LauncherView {
    LauncherView {
        title: "Rsynapse".to_owned(),
        subtitle: "Search applications, commands, shells, and more".to_owned(),
        placeholder: "Search".to_owned(),
        hints: default_hints(),
    }
}

fn plugin_key_event(event: &DbusKeyEvent) -> KeyEvent {
    let key = match event.key.as_str() {
        "Delete" => Key::Delete,
        "Backspace" => Key::Backspace,
        "Tab" => Key::Tab,
        "Enter" => Key::Enter,
        "Escape" => Key::Escape,
        other => Key::Other(other.to_owned()),
    };
    KeyEvent {
        key,
        ctrl: event.ctrl,
        alt: event.alt,
        shift: event.shift,
    }
}

// --- D-Bus interface ---

struct Engine {
    manager: Arc<PluginManager>,
    config: Arc<RwLock<DaemonConfig>>,
    execute_defaults: Arc<HashMap<String, String>>,
    last_search: Arc<Mutex<CachedSearch>>,
}

#[interface(name = "org.rsynapse.Engine1")]
impl Engine {
    async fn search(&self, request: SearchRequest) -> SearchResponse {
        let resolved = resolve_request(&self.manager, &request);
        let mut tagged: Vec<(f64, String, ResultItem)> = Vec::new();
        let mut plugin_views = Vec::new();

        for index in resolved.plugin_indices {
            let plugin = &self.manager.plugins[index];
            let plugin_name = plugin.name().to_owned();
            let context = QueryContext {
                group: resolved.focused_group,
                params: &resolved.params,
                query: &request.query,
                selected: non_empty(&request.selected),
            };
            let response = plugin.query(&context);
            if let Some(view) = response.view {
                plugin_views.push(view);
            }
            for item in response.results {
                tagged.push((item.score, plugin_name.clone(), item));
            }
        }

        tagged.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let mut cache = self.last_search.lock().unwrap();
        cache.results.clear();

        let results = tagged
            .into_iter()
            .map(|(_, plugin_name, item)| {
                let wire = launcher_result(item.clone());
                cache.results.push(CachedResult { plugin_name, item });
                wire
            })
            .collect();

        let view = resolved
            .focused_group
            .and_then(|_| plugin_views.into_iter().next())
            .map(launcher_view)
            .unwrap_or_else(|| match resolved.focused_group {
                Some(group_id) => self.manager.view_for_group(group_id),
                None => default_view(),
            });

        SearchResponse { view, results }
    }

    async fn describe(&self) -> DescribeResponse {
        DescribeResponse {
            default_view: default_view(),
            groups: self.manager.group_infos(),
        }
    }

    async fn handle_key(&self, request: KeyRequest) -> KeyResponse {
        let cached = {
            let cache = self.last_search.lock().unwrap();
            cache
                .results
                .iter()
                .find(|result| result.item.id == request.selected)
                .map(|result| (result.plugin_name.clone(), result.item.id.clone()))
        };
        let Some((plugin_name, selected)) = cached else {
            return KeyResponse::default();
        };
        let Some(plugin) = self
            .manager
            .plugins
            .iter()
            .find(|plugin| plugin.name() == plugin_name)
        else {
            return KeyResponse::default();
        };

        let (group, params) = key_request_context(&self.manager, &request.params);
        let context = QueryContext {
            group,
            params: &params,
            query: &request.query,
            selected: Some(selected.as_str()),
        };
        let event = plugin_key_event(&request.event);
        match plugin.handle_key(&event, &context) {
            KeyOutcome::Ignored => KeyResponse::default(),
            KeyOutcome::Handled {
                refresh,
                close,
                message,
            } => KeyResponse {
                handled: true,
                refresh,
                close,
                message: message.unwrap_or_default(),
            },
        }
    }

    async fn execute(&self, id: &str) -> String {
        let cached = {
            let cache = self.last_search.lock().unwrap();
            cache
                .results
                .iter()
                .find(|result| result.item.id == id)
                .map(|result| (result.plugin_name.clone(), result.item.clone()))
        };
        let Some((plugin_name, item)) = cached else {
            let msg = format!("No result with id '{}'", id);
            eprintln!("[Daemon] {}", msg);
            return format!("Error: {}", msg);
        };

        if let Some(plugin) = self
            .manager
            .plugins
            .iter()
            .find(|plugin| plugin.name() == plugin_name)
        {
            if let Some(result) = plugin.execute(&item) {
                return match result {
                    Ok(()) => String::new(),
                    Err(error) => {
                        eprintln!("[Daemon] {} execute failed: {}", plugin_name, error);
                        format!("Error: {}", error)
                    }
                };
            }
        }

        let config = self.config.read().unwrap();
        let config_execute = config
            .plugins
            .get(&plugin_name)
            .and_then(|cfg| cfg.execute.as_deref());

        let template = if let Some(tmpl) = config_execute {
            tmpl
        } else if let Some(default) = self.execute_defaults.get(&plugin_name) {
            default.as_str()
        } else {
            let msg = format!("No execute config for plugin '{}'", plugin_name);
            eprintln!("[Daemon] {}", msg);
            return format!("Error: {}", msg);
        };

        let command = template
            .replace("{id}", &item.id)
            .replace("{title}", &item.title)
            .replace("{description}", item.description.as_deref().unwrap_or(""))
            .replace("{icon}", item.icon.as_deref().unwrap_or(""))
            .replace("{data}", item.data.as_deref().unwrap_or(""));

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

fn launcher_view(view: rsynapse_plugin::PluginView) -> LauncherView {
    LauncherView {
        title: view.title,
        subtitle: view.subtitle.unwrap_or_default(),
        placeholder: view.placeholder,
        hints: view
            .hints
            .into_iter()
            .map(|hint| LauncherHint {
                key: hint.key,
                label: hint.label,
            })
            .collect(),
    }
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
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
        last_search: Arc::new(Mutex::new(CachedSearch::default())),
    };

    // Watch config file for changes.
    if let Some(config_dir) = dirs::config_dir() {
        let config_path = config_dir.join("rsynapse");
        let manager_ref = Arc::clone(&manager);
        let config_ref = Arc::clone(&config);

        std::thread::spawn(move || {
            use std::time::Instant;

            let (tx, rx) = std::sync::mpsc::channel();
            let mut watcher = notify::recommended_watcher(tx).unwrap();

            if config_path.exists() {
                watcher
                    .watch(&config_path, RecursiveMode::NonRecursive)
                    .unwrap();
                eprintln!("[Daemon] Watching config at {:?}", config_path);
            }

            let mut last_reload = Instant::now();
            for res in rx {
                match res {
                    Ok(event) => {
                        let is_toml = event
                            .paths
                            .iter()
                            .any(|p| p.extension().is_some_and(|ext| ext == "toml"));
                        let is_reload_event = matches!(
                            event.kind,
                            EventKind::Access(AccessKind::Close(AccessMode::Write))
                                | EventKind::Create(CreateKind::File)
                        );
                        if is_toml && is_reload_event && last_reload.elapsed().as_millis() > 100 {
                            last_reload = Instant::now();
                            eprintln!("[Daemon] Config changed, reloading...");
                            *config_ref.write().unwrap() = load_config();
                            for plugin in &manager_ref.plugins {
                                plugin.reload();
                            }
                        }
                    }
                    Err(e) => eprintln!("[Daemon] fsnotify error: {:?}", e),
                }
            }
        });
    }

    let _conn = ConnectionBuilder::session()?
        .name("org.rsynapse.Engine")?
        .serve_at("/org/rsynapse/Engine1", engine)?
        .build()
        .await?;

    println!("[Daemon] rsynapse server is running.");
    std::future::pending::<()>().await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsynapse_plugin::{KeyOutcome, PluginResponse, PluginView, QueryContext, ResultItem};

    struct TestPlugin {
        name: &'static str,
        groups: Vec<PluginGroup>,
    }

    impl Plugin for TestPlugin {
        fn name(&self) -> &'static str {
            self.name
        }

        fn groups(&self) -> Vec<PluginGroup> {
            self.groups.clone()
        }

        fn query(&self, _context: &QueryContext<'_>) -> PluginResponse {
            PluginResponse {
                view: None::<PluginView>,
                results: Vec::<ResultItem>::new(),
            }
        }

        fn handle_key(&self, _event: &KeyEvent, _context: &QueryContext<'_>) -> KeyOutcome {
            KeyOutcome::Ignored
        }
    }

    fn manager() -> PluginManager {
        let plugins = vec![
            TestPlugin {
                name: "workspaces",
                groups: vec![PluginGroup {
                    id: "workspaces".into(),
                    title: "Workspaces".into(),
                    description: None,
                    aliases: vec!["workspace".into(), "ws".into()],
                }],
            },
            TestPlugin {
                name: "apps",
                groups: vec![PluginGroup {
                    id: "apps".into(),
                    title: "Applications".into(),
                    description: None,
                    aliases: vec!["app".into()],
                }],
            },
        ];
        let mut manager = PluginManager::new();
        for plugin in plugins {
            let plugin_index = manager.plugins.len();
            for group in plugin.groups() {
                manager.groups.push(LoadedGroup {
                    group,
                    plugin_index,
                });
            }
            manager.plugins.push(Box::new(plugin));
        }
        manager
    }

    #[test]
    fn first_param_alias_focuses_group_and_forwards_remaining_params() {
        let manager = manager();
        let request = SearchRequest {
            params: vec!["ws".into(), "feature".into()],
            query: "q".into(),
            selected: String::new(),
        };
        let resolved = resolve_request(&manager, &request);
        assert_eq!(resolved.focused_group, Some("workspaces"));
        assert_eq!(resolved.params, vec!["feature"]);
        assert_eq!(resolved.plugin_indices, vec![0]);
    }

    #[test]
    fn unmatched_param_queries_all_plugins_and_forwards_params() {
        let manager = manager();
        let request = SearchRequest {
            params: vec!["unknown".into(), "filter".into()],
            query: "q".into(),
            selected: String::new(),
        };
        let resolved = resolve_request(&manager, &request);
        assert_eq!(resolved.focused_group, None);
        assert_eq!(resolved.params, request.params);
        assert_eq!(resolved.plugin_indices, vec![0, 1]);
    }

    #[test]
    fn group_infos_include_owning_plugin() {
        let groups = manager().group_infos();
        assert_eq!(groups[0].id, "workspaces");
        assert_eq!(groups[0].plugin, "workspaces");
        assert_eq!(groups[1].id, "apps");
        assert_eq!(groups[1].plugin, "apps");
    }

    #[test]
    fn delete_key_maps_to_the_plugin_key_event() {
        let event = plugin_key_event(&DbusKeyEvent {
            key: "Delete".into(),
            ctrl: false,
            alt: true,
            shift: true,
        });
        assert_eq!(event.key, Key::Delete);
        assert!(event.alt);
        assert!(event.shift);
    }
}
