//! Shared freedesktop desktop-entry parsing for shell consumers.
//!
//! Handles desktop IDs, `Hidden`/`NoDisplay`, `Exec` word splitting, and
//! percent codes in one place. Exec is always returned as argv; no shell is
//! involved.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopEntry {
    pub id: String,
    pub path: PathBuf,
    pub name: Option<String>,
    pub icon: Option<String>,
    pub startup_wm_class: Option<String>,
    pub terminal: bool,
    pub hidden: bool,
    pub no_display: bool,
    fields: BTreeMap<String, String>,
}

impl DesktopEntry {
    /// Parses a `[Desktop Entry]` group. Returns `None` when the group is absent.
    pub fn parse(contents: &str, path: &Path) -> Option<Self> {
        let mut entry = false;
        let mut fields = BTreeMap::new();
        for line in contents.lines().map(str::trim) {
            if line.starts_with('[') {
                entry = line == "[Desktop Entry]";
            } else if entry && !line.starts_with('#') {
                if let Some((key, value)) = line.split_once('=') {
                    fields.insert(key.to_owned(), value.to_owned());
                }
            }
        }
        if !entry && fields.is_empty() {
            return None;
        }
        let id = desktop_id_from_path(path);
        Some(Self {
            id,
            path: path.to_path_buf(),
            name: fields.get("Name").cloned(),
            icon: fields.get("Icon").cloned(),
            startup_wm_class: fields.get("StartupWMClass").cloned(),
            terminal: fields.get("Terminal").is_some_and(|value| value == "true"),
            hidden: fields.get("Hidden").is_some_and(|value| value == "true"),
            no_display: fields.get("NoDisplay").is_some_and(|value| value == "true"),
            fields,
        })
    }

    pub fn field(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(String::as_str)
    }

    /// Whether this entry should be offered in launchers/menus.
    pub fn is_visible(&self) -> bool {
        !self.hidden && !self.no_display
    }

    /// Exec argv with desktop-entry percent codes expanded. Returns `None` when
    /// the entry has no valid `Exec` or still contains an unsupported code.
    pub fn exec_argv(&self, app_fallback: &str) -> Option<Vec<String>> {
        let exec = self.field("Exec")?;
        let name = self.name.as_deref().unwrap_or(app_fallback);
        let path = self.path.to_string_lossy();
        let mut argv = Vec::new();
        for argument in shlex::split(exec)? {
            if [
                "%f", "%F", "%u", "%U", "%i", "%d", "%D", "%n", "%N", "%v", "%m",
            ]
            .contains(&argument.as_str())
            {
                continue;
            }
            let argument = argument
                .replace("%c", name)
                .replace("%k", &path)
                .replace("%%", "\u{1}");
            if argument.contains('%') {
                return None;
            }
            argv.push(argument.replace('\u{1}', "%"));
        }
        (!argv.is_empty()).then_some(argv)
    }
}

/// Desktop ID for a `.desktop` path: subdirectories become `-` separators.
pub fn desktop_id_from_path(path: &Path) -> String {
    let components: Vec<&str> = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect();
    if let Some(index) = components.iter().rposition(|name| *name == "applications") {
        return components[index + 1..]
            .join("-")
            .trim_end_matches(".desktop")
            .to_owned();
    }
    components
        .last()
        .map(|name| name.trim_end_matches(".desktop"))
        .unwrap_or_default()
        .to_owned()
}

/// Searches `XDG_DATA_HOME` and `XDG_DATA_DIRS` for `<app>.desktop`.
pub fn find(app: &str) -> Option<DesktopEntry> {
    if app.is_empty() || app.contains('/') {
        return None;
    }
    let home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
        })?;
    let mut directories = vec![home];
    directories.extend(std::env::split_paths(
        &std::env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into()),
    ));
    let path = directories
        .into_iter()
        .map(|dir| dir.join("applications").join(format!("{app}.desktop")))
        .find(|path| path.is_file())?;
    let contents = std::fs::read_to_string(&path).ok()?;
    DesktopEntry::parse(&contents, &path)
}

/// Desktop ID for a relative applications path, e.g. `kde/foo.desktop` ->
/// `kde-foo.desktop` without the extension.
pub fn desktop_id_from_applications_path(path: &str) -> String {
    path.trim_start_matches('/')
        .trim_end_matches(".desktop")
        .replace('/', "-")
}

#[cfg(test)]
mod tests {
    use super::{DesktopEntry, desktop_id_from_applications_path};
    use std::path::Path;

    #[test]
    fn parses_exec_into_argv_without_a_shell() {
        let entry = DesktopEntry::parse(
            "[Desktop Entry]\nName=Browser\nExec=browser --new-window %u\n",
            Path::new("/x/browser.desktop"),
        )
        .unwrap();
        assert_eq!(
            entry.exec_argv("browser"),
            Some(vec!["browser".to_owned(), "--new-window".to_owned()])
        );
        assert!(entry.is_visible());
    }

    #[test]
    fn hidden_and_nodisplay_are_not_visible() {
        let hidden = DesktopEntry::parse(
            "[Desktop Entry]\nName=X\nExec=x\nHidden=true\n",
            Path::new("/x/x.desktop"),
        )
        .unwrap();
        let nodisplay = DesktopEntry::parse(
            "[Desktop Entry]\nName=X\nExec=x\nNoDisplay=true\n",
            Path::new("/x/x.desktop"),
        )
        .unwrap();
        assert!(!hidden.is_visible());
        assert!(!nodisplay.is_visible());
    }

    #[test]
    fn quotes_and_percent_codes_are_argv_safe() {
        let entry = DesktopEntry::parse(
            "[Desktop Entry]\nName=My App\nExec=app --title=%c --path=%k %U\n",
            Path::new("/x/my.desktop"),
        )
        .unwrap();
        let argv = entry.exec_argv("fallback").unwrap();
        assert_eq!(argv[0], "app");
        assert!(argv.iter().any(|arg| arg.contains("My App")));
        assert!(!argv.join(" ").contains("%"));
    }

    #[test]
    fn desktop_ids_use_dash_separators() {
        assert_eq!(
            desktop_id_from_applications_path("kde/org.kde.app.desktop"),
            "kde-org.kde.app"
        );
    }
}
