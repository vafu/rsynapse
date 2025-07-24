use freedesktop_desktop_entry::{DesktopEntry, Iter};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use rsynapse_plugin::{Plugin, ResultItem};
use std::path::PathBuf;
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

/// The main struct for our plugin. It holds the list of all applications
/// found on the system.
struct LauncherPlugin {
    apps: Vec<App>,
    matcher: SkimMatcherV2,
}

impl LauncherPlugin {
    /// Creates a new instance of the plugin, scanning and indexing all apps.
    fn new() -> Self {
        let apps = Self::find_and_parse_apps();
        println!("[LauncherPlugin] Indexed {} applications.", apps.len());
        Self {
            apps,
            matcher: SkimMatcherV2::default(),
        }
    }

    /// Scans XDG standard directories for .desktop files and parses them.
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
                if let Ok(app) = Self::parse_desktop_file(entry.path()) {
                    apps.push(app);
                }
            }
        }
        apps
    }

    /// Parses a single .desktop file into our App struct.
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
}

impl Plugin for LauncherPlugin {
    fn name(&self) -> &'static str {
        "Application Launcher"
    }

    fn query(&self, query: &str) -> Vec<ResultItem> {
        if query.is_empty() {
            return Vec::new();
        }

        self.apps
            .iter()
            .filter_map(|app| {
                // Match against the app name. A better implementation would also match keywords, etc.
                self.matcher
                    .fuzzy_match(&app.name, query)
                    .map(|score| (score, app))
            })
            .map(|(_score, app)| ResultItem {
                id: app.desktop_file_id.clone(),
                title: app.name.clone(),
                description: app.comment.clone(),
                icon: app.icon.clone(),
                command: app.exec.clone(),
            })
            .collect()
    }
}

/// The plugin's FFI-safe entry point.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _rsynapse_init() -> *mut dyn Plugin {
    Box::into_raw(Box::new(LauncherPlugin::new()))
}
