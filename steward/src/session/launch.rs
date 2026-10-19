//! Small launch providers, not a plugin framework. Recipes are versioned so
//! future content-restoration providers can extend this without replacing it.
use super::model::Recipe;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub const GHOSTTY_PREFIX: &str = "com.mitchellh.ghostty.rsynapse.r";

#[derive(Clone, Default, serde::Deserialize)]
pub struct Rules(pub BTreeMap<String, Rule>);
#[derive(Clone, serde::Deserialize)]
pub struct Rule {
    pub argv: Vec<String>,
    pub match_app_id: String,
}
impl Rules {
    pub fn load() -> anyhow::Result<Self> {
        let path = std::env::var_os("STEWARD_SESSION_LAUNCHERS_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var_os("XDG_CONFIG_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        PathBuf::from(std::env::var_os("HOME").unwrap()).join(".config")
                    })
                    .join("rsynapse/session-launchers.json")
            });
        match std::fs::read(path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }
    pub fn recipe(&self, app: &str, cwd: &str) -> Option<Recipe> {
        let app = canonical(app);
        if let Some(rule) = self.0.get(app) {
            return Some(Recipe {
                provider: "configured".into(),
                version: 1,
                argv: rule.argv.clone(),
                cwd: cwd.into(),
                match_app_id: rule.match_app_id.clone(),
            });
        }
        if app == "com.mitchellh.ghostty" {
            return Some(Recipe {
                provider: "ghostty".into(),
                version: 1,
                cwd: cwd.into(),
                argv: vec![
                    "ghostty".into(),
                    "--gtk-single-instance=false".into(),
                    format!("--class={GHOSTTY_PREFIX}{{restore_id}}"),
                    "--window-inherit-working-directory=false".into(),
                    "--working-directory={cwd}".into(),
                ],
                match_app_id: format!("{GHOSTTY_PREFIX}{{restore_id}}"),
            });
        }
        desktop(app, cwd)
    }
}

pub fn canonical(app: &str) -> &str {
    if app.starts_with(GHOSTTY_PREFIX) {
        "com.mitchellh.ghostty"
    } else {
        app
    }
}

fn desktop(app: &str, cwd: &str) -> Option<Recipe> {
    if app.is_empty() || app.contains('/') {
        return None;
    }
    let home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap()).join(".local/share"));
    let mut directories = vec![home];
    directories.extend(std::env::split_paths(
        &std::env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into()),
    ));
    let path = directories
        .into_iter()
        .map(|p| p.join("applications").join(format!("{app}.desktop")))
        .find(|p| p.is_file())?;
    let contents = std::fs::read_to_string(&path).ok()?;
    desktop_recipe(&contents, &path, app, cwd)
}

fn desktop_recipe(contents: &str, path: &Path, app: &str, cwd: &str) -> Option<Recipe> {
    let mut entry = false;
    let mut fields = BTreeMap::new();
    for line in contents.lines().map(str::trim) {
        if line.starts_with('[') {
            entry = line == "[Desktop Entry]";
        } else if entry && !line.starts_with('#') {
            if let Some((key, value)) = line.split_once('=') {
                fields.insert(key, value);
            }
        }
    }
    if fields.get("Hidden") == Some(&"true") || fields.get("Terminal") == Some(&"true") {
        return None;
    }
    let mut argv = Vec::new();
    for argument in shlex::split(fields.get("Exec")?)? {
        if ["%f", "%F", "%u", "%U", "%i"].contains(&argument.as_str()) {
            continue;
        }
        let argument = argument
            .replace("%c", fields.get("Name").copied().unwrap_or(app))
            .replace("%k", &path.to_string_lossy())
            .replace("%%", "\u{1}");
        if argument.contains('%') {
            return None;
        }
        argv.push(argument.replace('\u{1}', "%"));
    }
    (!argv.is_empty()).then(|| Recipe {
        provider: "desktop".into(),
        version: 1,
        argv,
        cwd: cwd.into(),
        match_app_id: app.into(),
    })
}

pub fn expanded(recipe: &Recipe, identity: &str) -> anyhow::Result<(Vec<String>, String)> {
    anyhow::ensure!(recipe.version == 1, "unsupported launch recipe version");
    anyhow::ensure!(
        ["ghostty", "desktop", "configured"].contains(&recipe.provider.as_str()),
        "unsupported launch provider"
    );
    anyhow::ensure!(
        Path::new(&recipe.cwd).is_dir(),
        "launch CWD no longer exists: {}",
        recipe.cwd
    );
    let expand = |value: &str| {
        value
            .replace("{restore_id}", identity)
            .replace("{cwd}", &recipe.cwd)
    };
    let argv: Vec<_> = recipe.argv.iter().map(|v| expand(v)).collect();
    anyhow::ensure!(
        !argv.is_empty() && !argv[0].is_empty(),
        "empty launch command"
    );
    Ok((argv, expand(&recipe.match_app_id)))
}

/// A fresh, direct process/descendant is causal launch evidence for desktop
/// entries. Single-instance apps that forward to an older process do not match.
pub fn descendant(mut pid: i32, ancestor: u32) -> bool {
    for _ in 0..64 {
        if pid > 0 && pid as u32 == ancestor {
            return true;
        }
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
            return false;
        };
        let Some((_, tail)) = stat.rsplit_once(')') else {
            return false;
        };
        let Some(parent) = tail
            .split_whitespace()
            .nth(1)
            .and_then(|p| p.parse::<i32>().ok())
        else {
            return false;
        };
        if parent <= 1 || parent == pid {
            return false;
        }
        pid = parent;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_arguments_are_not_executed_through_a_shell() {
        let recipe = desktop_recipe(
            "[Desktop Entry]\nName=An App\nExec=app --name %c %U 'literal;argument'\n",
            Path::new("/app.desktop"),
            "app",
            "/tmp",
        )
        .unwrap();
        assert_eq!(recipe.argv, ["app", "--name", "An App", "literal;argument"]);
        assert!(
            desktop_recipe(
                "[Desktop Entry]\nExec=app %unsupported\n",
                Path::new("/a"),
                "app",
                "/tmp"
            )
            .is_none()
        );
    }
}
