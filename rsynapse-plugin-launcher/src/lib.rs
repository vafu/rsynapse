use freedesktop_desktop_entry::{DesktopEntry, Iter};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use notify::{RecursiveMode, Watcher};
use rsynapse_plugin::{Plugin, ResultItem};
use std::{
    collections::HashSet,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};
use walkdir::WalkDir;
use xdg::BaseDirectories;

/// A struct to hold the indexed information for a single application.
#[derive(Debug, Clone)]
struct App {
    name: String,
    comment: Option<String>,
    exec: Option<String>,
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

fn find_and_parse_apps() -> Vec<App> {
    let mut apps = Vec::new();
    let xdg_dirs = BaseDirectories::new().unwrap();

    // Get all application directories
    let mut app_dirs: Vec<PathBuf> = xdg_dirs.get_data_dirs();
    app_dirs.push(xdg_dirs.get_data_home());

    for dir in app_dirs {
        let app_dir = dir.join("applications");
        if !app_dir.exists() {
            continue;
        }

        // Walk the directory to find .desktop files
        for entry in WalkDir::new(app_dir)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "desktop"))
        {
            if let Ok(app) = parse_desktop_file(entry.path()) {
                apps.push(app);
            }
        }
    }
    apps
}

fn parse_desktop_file(path: &std::path::Path) -> Result<App, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let entry = DesktopEntry::decode(path, &content)?;

    // Ignore hidden entries
    if entry.no_display() {
        return Err("Hidden entry".into());
    }

    // We only care about "Application" types
    if entry.type_() != Some("Application") {
        return Err("Not an application".into());
    }

    let name = entry.name(None).ok_or("No name")?.to_string();
    let comment = entry.comment(None).map(|s| s.to_string());
    let exec = entry.exec().map(|s| s.to_string());
    let icon = entry.icon().map(|s| s.to_string());
    let desktop_file_id = path.file_name().unwrap().to_string_lossy().to_string();

    Ok(App {
        name,
        comment,
        exec,
        icon,
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

    fn query(&self, query: &str) -> Vec<ResultItem> {
        if query.is_empty() {
            return Vec::new();
        }

        let index = self.index.lock().unwrap();

        index
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
                description: app.comment.clone(),
                icon: app.icon.clone(),
                command: app.exec.clone(),
                // Pass the score from the fuzzy matcher.
                score: score as f64,
            })
            .collect()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _rsynapse_init() -> *mut dyn Plugin {
    let index = Arc::new(Mutex::new(AppIndex::new()));
    reindex(&mut index.lock().unwrap());
    start_watcher_thread(Arc::clone(&index));
    Box::into_raw(Box::new(LauncherPlugin { index }))
}
