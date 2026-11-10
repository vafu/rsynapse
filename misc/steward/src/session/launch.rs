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
                goal_model::xdg::config_home()
                    .unwrap_or_else(|_| PathBuf::from(".config"))
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
    let entry = desktop_entry::find(app)?;
    if !entry.is_visible() || entry.terminal {
        return None;
    }
    Some(Recipe {
        provider: "desktop".into(),
        version: 1,
        argv: entry.exec_argv(app)?,
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
        let entry = desktop_entry::DesktopEntry::parse(
            "[Desktop Entry]\nName=An App\nExec=app --name %c %U 'literal;argument'\n",
            Path::new("/app.desktop"),
        )
        .unwrap();
        assert_eq!(
            entry.exec_argv("app").unwrap(),
            ["app", "--name", "An App", "literal;argument"]
        );
        let unsupported = desktop_entry::DesktopEntry::parse(
            "[Desktop Entry]\nExec=app %unsupported\n",
            Path::new("/a"),
        )
        .unwrap();
        assert!(unsupported.exec_argv("app").is_none());
    }
}
