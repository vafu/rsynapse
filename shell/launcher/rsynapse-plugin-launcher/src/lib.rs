use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use notify::{RecursiveMode, Watcher};
use rsynapse_plugin::{Plugin, PluginGroup, PluginResponse, QueryContext, ResultItem};
use std::{
    collections::HashSet,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};
use xdg::BaseDirectories;

/// A struct to hold the indexed information for a single application.
#[derive(Debug, Clone)]
struct App {
    name: String,
    comment: Option<String>,
    argv: Vec<String>,
    icon: Option<String>,
    desktop_file_id: String,
}

struct AppIndex {
    data_dirs: Vec<PathBuf>,
    apps: Vec<App>,
    matcher: SkimMatcherV2,
}

impl AppIndex {
    fn new() -> Self {
        let mut data_dirs = Vec::new();
        if let Ok(xdg_dirs) = BaseDirectories::new() {
            data_dirs.push(xdg_dirs.get_data_home());
            data_dirs.extend(xdg_dirs.get_data_dirs());
            data_dirs = data_dirs
                .into_iter()
                .map(|p| p.join("applications"))
                .collect();
        }
        Self {
            apps: Vec::new(),
            matcher: SkimMatcherV2::default(),
            data_dirs: data_dirs,
        }
    }
}

pub struct LauncherPlugin {
    index: Arc<Mutex<AppIndex>>,
}

fn reindex(index: &mut AppIndex) {
    eprintln!("[Launcher Plugin] Re-indexing applications...");
    let mut apps = Vec::new();
    let mut seen_ids = HashSet::<String>::new();

    for dir in &index.data_dirs {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(app) = parse_desktop_file(&entry.path()) {
                    if seen_ids.insert(app.desktop_file_id.clone()) {
                        apps.push(app);
                    }
                }
            }
        }
    }
    index.apps = apps;
    eprintln!(
        "[Launcher Plugin] Re-indexing complete. Found {} applications.",
        index.apps.len()
    );
}

fn parse_desktop_file(path: &std::path::Path) -> Result<App, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let entry = desktop_entry::DesktopEntry::parse(&content, path).ok_or("No desktop entry")?;
    if !entry.is_visible() || entry.terminal {
        return Err("Hidden entry".into());
    }
    if entry.field("Type") != Some("Application") {
        return Err("Not an application".into());
    }
    let name = entry.name.clone().ok_or("No name")?;
    let argv = entry.exec_argv(&name).ok_or("No exec")?;
    let desktop_file_id = entry.id.clone();
    Ok(App {
        name,
        comment: entry.field("Comment").map(str::to_owned),
        argv,
        icon: entry.icon.clone(),
        desktop_file_id,
    })
}

fn start_watcher_thread(index: Arc<Mutex<AppIndex>>) {
    thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = notify::recommended_watcher(tx).unwrap();

        for dir in &index.lock().unwrap().data_dirs {
            if dir.exists() {
                eprintln!("[Launcher Plugin] Watching for changes in: {:?}", dir);
                watcher.watch(&dir, RecursiveMode::NonRecursive).unwrap();
            }
        }

        for res in rx {
            if let Ok(_event) = res {
                let mut index_guard = index.lock().unwrap();
                reindex(&mut index_guard);
            }
        }
    });
}

impl Plugin for LauncherPlugin {
    fn name(&self) -> &'static str {
        "Application Launcher"
    }

    fn groups(&self) -> Vec<PluginGroup> {
        vec![PluginGroup {
            id: "apps".to_owned(),
            title: "Applications".to_owned(),
            description: "Launch installed applications".to_owned(),
            aliases: vec![
                "app".to_owned(),
                "applications".to_owned(),
                "launch".to_owned(),
            ],
            plugin: String::new(),
        }]
    }

    fn default_execute(&self) -> Option<&'static str> {
        None
    }

    fn execute(&self, item: &ResultItem) -> Option<Result<(), String>> {
        let index = self.index.lock().unwrap();
        let app = index
            .apps
            .iter()
            .find(|app| app.desktop_file_id == item.id)?;
        let Some((program, args)) = app.argv.split_first() else {
            return Some(Err("desktop entry has no exec".to_owned()));
        };
        match std::process::Command::new(program).args(args).spawn() {
            Ok(_) => Some(Ok(())),
            Err(error) => Some(Err(format!("failed to launch {program}: {error}"))),
        }
    }

    fn query(&self, context: &QueryContext<'_>) -> PluginResponse {
        let query = context.query;
        if query.is_empty() {
            return PluginResponse::default();
        }

        let index = self.index.lock().unwrap();

        let results = index
            .apps
            .iter()
            .filter_map(|app| {
                index
                    .matcher
                    .fuzzy_match(&app.name, query)
                    .map(|score| (score, app))
            })
            .map(|(score, app)| ResultItem {
                id: app.desktop_file_id.clone(),
                title: app.name.clone(),
                description: app.comment.clone().unwrap_or_default(),
                icon: app.icon.clone().unwrap_or_default(),
                data: String::new(),
                score: score as f64,
                current: false,
                keep_open: false,
            })
            .collect();

        PluginResponse {
            view: None,
            results,
        }
    }
}

pub fn plugin() -> Box<dyn Plugin> {
    let index = Arc::new(Mutex::new(AppIndex::new()));
    reindex(&mut index.lock().unwrap());
    start_watcher_thread(Arc::clone(&index));
    Box::new(LauncherPlugin { index })
}
