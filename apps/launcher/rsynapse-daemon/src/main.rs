use anyhow::Result;
use notify::{
    EventKind, RecursiveMode, Watcher,
    event::{AccessKind, AccessMode, CreateKind},
};
use rsynapse_launcher_api::{
    DescribeResponse, KeyRequest, KeyResponse, LauncherPlugin, SearchRequest, SearchResponse,
};
use rsynapse_plugin::{Plugin, PluginScope, PluginSpec, QueryContext, QueryMode, ResultItem};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use zbus::{ConnectionBuilder, interface};

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

struct LoadedPlugin {
    spec: PluginSpec,
    plugin_index: usize,
}

struct PluginManager {
    plugins: Vec<Box<dyn Plugin>>,
    entries: Vec<LoadedPlugin>,
}

impl PluginManager {
    fn with_builtins(plugins: Vec<Box<dyn Plugin>>) -> Self {
        let mut manager = Self {
            plugins: Vec::new(),
            entries: Vec::new(),
        };
        for plugin in plugins {
            let index = manager.plugins.len();
            let spec = plugin.spec();
            manager.entries.push(LoadedPlugin {
                spec,
                plugin_index: index,
            });
            manager.plugins.push(plugin);
        }
        manager
    }

    fn resolve_plugin(&self, param: &str) -> Option<&LoadedPlugin> {
        self.entries.iter().find(|entry| {
            entry.spec.scope != PluginScope::MainOnly
                && (entry.spec.id.eq_ignore_ascii_case(param)
                    || entry
                        .spec
                        .aliases
                        .iter()
                        .any(|alias| alias.eq_ignore_ascii_case(param)))
        })
    }

    fn plugin_infos(&self) -> Vec<LauncherPlugin> {
        self.entries
            .iter()
            .map(|entry| entry.spec.to_wire())
            .collect()
    }
}

struct ResolvedRequest<'a> {
    direct: Option<&'a LoadedPlugin>,
    params: Vec<String>,
    plugin_indices: Vec<usize>,
}

fn resolve_request<'a>(manager: &'a PluginManager, request: &SearchRequest) -> ResolvedRequest<'a> {
    if let Some(first) = request.params.first()
        && let Some(entry) = manager.resolve_plugin(first)
    {
        return ResolvedRequest {
            direct: Some(entry),
            params: request.params[1..].to_vec(),
            plugin_indices: vec![entry.plugin_index],
        };
    }
    ResolvedRequest {
        direct: None,
        params: request.params.clone(),
        plugin_indices: manager
            .entries
            .iter()
            .filter(|entry| entry.spec.scope != PluginScope::DirectOnly)
            .map(|entry| entry.plugin_index)
            .collect(),
    }
}

#[derive(Debug, Clone, Default)]
struct CachedSearch {
    results: Vec<CachedResult>,
}

#[derive(Debug, Clone)]
struct CachedResult {
    plugin_name: String,
    item: ResultItem,
}

fn run_search(manager: &PluginManager, request: SearchRequest) -> (SearchResponse, CachedSearch) {
    let resolved = resolve_request(manager, &request);
    let mode = if resolved.direct.is_some() {
        QueryMode::Direct
    } else {
        QueryMode::Main
    };
    let mut tagged: Vec<(f64, String, ResultItem)> = Vec::new();
    let mut summary = String::new();
    let mut actions = Vec::new();

    for index in resolved.plugin_indices {
        let plugin = &manager.plugins[index];
        let plugin_name = plugin.spec().name.to_owned();
        let context = QueryContext {
            mode,
            params: &resolved.params,
            query: &request.query,
            selected: non_empty(&request.selected),
        };
        let response = plugin.query(&context);
        if summary.is_empty() {
            summary = response.summary;
        }
        if actions.is_empty() {
            actions = response.actions;
        }
        for item in response.results {
            tagged.push((item.score, plugin_name.clone(), item));
        }
    }
    tagged.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut cached = CachedSearch::default();
    let results = tagged
        .into_iter()
        .map(|(_, plugin_name, item)| {
            let wire = item.clone();
            cached.results.push(CachedResult { plugin_name, item });
            wire
        })
        .collect();
    let (plugin, plugin_name) = match resolved.direct {
        Some(entry) => (entry.spec.id.to_owned(), entry.spec.name.to_owned()),
        None => (String::new(), String::new()),
    };
    if resolved.direct.is_none() {
        summary.clear();
        actions.clear();
    }
    (
        SearchResponse {
            plugin,
            plugin_name,
            summary,
            actions,
            results,
        },
        cached,
    )
}

fn key_context<'a>(manager: &'a PluginManager, params: &'a [String]) -> (QueryMode, Vec<String>) {
    if let Some(first) = params.first()
        && manager.resolve_plugin(first).is_some()
    {
        return (QueryMode::Direct, params[1..].to_vec());
    }
    (QueryMode::Main, params.to_vec())
}

fn run_key(manager: &PluginManager, cache: &CachedSearch, request: KeyRequest) -> KeyResponse {
    let cached = cache
        .results
        .iter()
        .find(|result| result.item.id == request.selected)
        .map(|result| (result.plugin_name.clone(), result.item.id.clone()));
    let Some((plugin_name, selected)) = cached else {
        return KeyResponse::default();
    };
    let Some(plugin) = manager
        .plugins
        .iter()
        .find(|plugin| plugin.spec().name == plugin_name)
    else {
        return KeyResponse::default();
    };
    let (mode, params) = key_context(manager, &request.params);
    plugin.handle_key(
        &request.event,
        &QueryContext {
            mode,
            params: &params,
            query: &request.query,
            selected: Some(selected.as_str()),
        },
    )
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

struct Engine {
    manager: Arc<PluginManager>,
    config: Arc<RwLock<DaemonConfig>>,
    execute_defaults: Arc<HashMap<String, String>>,
    last_search: Arc<Mutex<CachedSearch>>,
}

#[interface(name = "org.rsynapse.Engine1")]
impl Engine {
    async fn search(&self, request: SearchRequest) -> SearchResponse {
        let manager = self.manager.clone();
        let (response, cached) = tokio::task::spawn_blocking(move || run_search(&manager, request))
            .await
            .unwrap_or_else(|error| {
                eprintln!("[Daemon] search worker failed: {error}");
                (SearchResponse::default(), CachedSearch::default())
            });
        *self.last_search.lock().unwrap() = cached;
        response
    }

    async fn describe(&self) -> DescribeResponse {
        DescribeResponse {
            plugins: self.manager.plugin_infos(),
            abi_version: rsynapse_launcher_api::PLUGIN_ABI_VERSION,
        }
    }

    async fn handle_key(&self, request: KeyRequest) -> KeyResponse {
        let manager = self.manager.clone();
        let cache = self.last_search.lock().unwrap().clone();
        tokio::task::spawn_blocking(move || run_key(&manager, &cache, request))
            .await
            .unwrap_or_else(|error| {
                eprintln!("[Daemon] key worker failed: {error}");
                KeyResponse::default()
            })
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
        let plugin_result = {
            let manager = self.manager.clone();
            let plugin_name = plugin_name.clone();
            let item = item.clone();
            tokio::task::spawn_blocking(move || {
                manager
                    .plugins
                    .iter()
                    .find(|plugin| plugin.spec().name == plugin_name)
                    .and_then(|plugin| plugin.execute(&item))
            })
            .await
            .ok()
            .flatten()
        };
        if let Some(result) = plugin_result {
            return match result {
                Ok(()) => String::new(),
                Err(error) => {
                    eprintln!("[Daemon] {} execute failed: {}", plugin_name, error);
                    format!("Error: {}", error)
                }
            };
        }
        let template = {
            let config = self.config.read().unwrap();
            let config_execute = config
                .plugins
                .get(&plugin_name)
                .and_then(|cfg| cfg.execute.clone());
            if let Some(template) = config_execute {
                template
            } else if let Some(default) = self.execute_defaults.get(&plugin_name) {
                default.clone()
            } else {
                let msg = format!("No execute config for plugin '{}'", plugin_name);
                eprintln!("[Daemon] {}", msg);
                return format!("Error: {}", msg);
            }
        };
        let output = template
            .replace("{id}", &shell_quote(&item.id))
            .replace("{title}", &shell_quote(&item.title))
            .replace("{description}", &shell_quote(&item.description))
            .replace("{icon}", &shell_quote(&item.icon))
            .replace("{data}", &shell_quote(&item.data));
        run_execute(&output)
    }
}

fn run_execute(command: &str) -> String {
    match std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
    {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let msg = if stderr.trim().is_empty() {
                format!("command exited with {}", output.status)
            } else {
                stderr.trim().to_owned()
            };
            eprintln!("[Daemon] execute failed: {}", msg);
            format!("Error: {}", msg)
        }
        Err(error) => {
            eprintln!("[Daemon] execute failed: {}", error);
            format!("Error: {error}")
        }
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn builtin_plugins() -> Vec<Box<dyn Plugin>> {
    let mut plugins = vec![
        rsynapse_plugin_audio::plugin(),
        rsynapse_plugin_bluetooth::plugin(),
        rsynapse_plugin_calc::plugin(),
        rsynapse_plugin_commands::plugin(),
        rsynapse_plugin_launcher::plugin(),
        rsynapse_plugin_shell::plugin(),
    ];
    plugins.extend(rsynapse_plugin_workspaces::plugins());
    plugins
}

#[tokio::main]
async fn main() -> Result<()> {
    let manager = PluginManager::with_builtins(builtin_plugins());
    let config = load_config();
    eprintln!("[Daemon] Loaded {} plugin(s).", manager.plugins.len());
    let mut execute_defaults = HashMap::new();
    for plugin in &manager.plugins {
        if let Some(default) = plugin.default_execute() {
            execute_defaults.insert(plugin.spec().name.to_string(), default.to_string());
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

    let manager_ref = Arc::clone(&manager);
    let config_ref = Arc::clone(&config);
    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let Ok(mut watcher) = notify::recommended_watcher(tx) else {
            return;
        };
        let Some(config_dir) = dirs::config_dir() else {
            return;
        };
        let rsynapse_dir = config_dir.join("rsynapse");
        let _ = std::fs::create_dir_all(&rsynapse_dir);
        if watcher
            .watch(&rsynapse_dir, RecursiveMode::Recursive)
            .is_err()
        {
            return;
        }
        let mut last_reload = std::time::Instant::now();
        for event in rx {
            let Ok(event) = event else {
                continue;
            };
            let is_toml = event
                .paths
                .iter()
                .any(|p| p.extension().is_some_and(|e| e == "toml"));
            let is_reload_event = matches!(
                event.kind,
                EventKind::Modify(_)
                    | EventKind::Create(CreateKind::File)
                    | EventKind::Access(AccessKind::Close(AccessMode::Write))
            );
            if is_toml && is_reload_event && last_reload.elapsed().as_millis() > 100 {
                last_reload = std::time::Instant::now();
                eprintln!("[Daemon] Config changed, reloading...");
                *config_ref.write().unwrap() = load_config();
                for plugin in &manager_ref.plugins {
                    plugin.reload();
                }
            }
        }
    });

    let _conn = ConnectionBuilder::session()?
        .name("org.rsynapse.Engine")?
        .serve_at("/org/rsynapse/Engine1", engine)?
        .build()
        .await?;
    eprintln!("[Daemon] rsynapse server is running.");
    std::future::pending::<()>().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsynapse_launcher_api::KeyEvent;
    use rsynapse_plugin::{PluginResponse, PluginSpec, QueryMode};

    struct TestPlugin {
        spec: PluginSpec,
    }
    impl Plugin for TestPlugin {
        fn spec(&self) -> PluginSpec {
            self.spec
        }
        fn query(&self, _context: &QueryContext<'_>) -> PluginResponse {
            PluginResponse::default()
        }
    }

    fn manager() -> PluginManager {
        PluginManager::with_builtins(vec![
            Box::new(TestPlugin {
                spec: PluginSpec {
                    id: "workspaces",
                    name: "Workspaces",
                    aliases: &["ws"],
                    scope: PluginScope::DirectOnly,
                },
            }),
            Box::new(TestPlugin {
                spec: PluginSpec {
                    id: "apps",
                    name: "Applications",
                    aliases: &["app"],
                    scope: PluginScope::Both,
                },
            }),
        ])
    }

    #[test]
    fn alias_focuses_direct_plugin() {
        let manager = manager();
        let resolved = resolve_request(
            &manager,
            &SearchRequest {
                params: vec!["ws".into(), "feature".into()],
                query: "q".into(),
                selected: String::new(),
            },
        );
        assert_eq!(resolved.direct.map(|e| e.spec.id), Some("workspaces"));
        assert_eq!(resolved.params, vec!["feature"]);
        assert_eq!(resolved.plugin_indices, vec![0]);
    }

    #[test]
    fn main_search_skips_direct_only_plugins() {
        let manager = manager();
        let request = SearchRequest {
            params: vec!["unknown".into()],
            query: "q".into(),
            selected: String::new(),
        };
        let resolved = resolve_request(&manager, &request);
        assert!(resolved.direct.is_none());
        assert_eq!(resolved.plugin_indices, vec![1]);
    }

    #[test]
    fn describe_reports_plugin_specs() {
        let infos = manager().plugin_infos();
        assert_eq!(infos[0].id, "workspaces");
        assert_eq!(infos[1].scope, PluginScope::Both);
    }

    #[test]
    fn key_context_uses_direct_mode() {
        let manager = manager();
        let (mode, params) = key_context(&manager, &["ws".into(), "filter".into()]);
        assert_eq!(mode, QueryMode::Direct);
        assert_eq!(params, vec!["filter"]);
    }

    #[test]
    fn key_events_use_shared_wire_type() {
        let event = KeyEvent {
            key: "Delete".into(),
            ctrl: false,
            alt: true,
            shift: true,
        };
        assert_eq!(event.key, "Delete");
    }

    #[test]
    fn static_registry_covers_all_builtin_plugins() {
        let names = builtin_plugins()
            .iter()
            .map(|plugin| plugin.spec().name)
            .collect::<Vec<_>>();
        for expected in [
            "Audio Output",
            "Bluetooth",
            "Calculator",
            "Commands",
            "Applications",
            "Shell",
            "Projects",
            "Workspaces",
        ] {
            assert!(
                names.contains(&expected),
                "missing plugin {expected}: {names:?}"
            );
        }
    }
}
