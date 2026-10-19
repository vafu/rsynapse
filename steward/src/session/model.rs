use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

pub fn id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("OS random identity source");
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct Recipe {
    pub provider: String,
    pub version: u32,
    pub argv: Vec<String>,
    pub cwd: String,
    /// `{restore_id}` is a launch-specific identity, not a saved compositor ID.
    pub match_app_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct Window {
    pub id: String,
    pub app_id: String,
    pub floating: bool,
    pub column: Vec<u64>,
    pub row: Vec<u64>,
    pub recipe: Vec<Recipe>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct Workspace {
    pub id: String,
    /// Immutable checkpoint relation key; its project reference lives in Locus.
    pub binding: String,
    pub output: String,
    pub index: u32,
    pub name: String,
    pub layout: workspace_dbus_api::Layout,
    pub windows: Vec<Window>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct Session {
    pub version: u32,
    pub name: String,
    pub workspaces: Vec<Workspace>,
}

impl Session {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.version == 1,
            "unsupported session schema {}",
            self.version
        );
        anyhow::ensure!(!self.name.is_empty(), "session name is empty");
        let mut ids = std::collections::HashSet::new();
        for workspace in &self.workspaces {
            anyhow::ensure!(
                !workspace.id.is_empty() && ids.insert(&workspace.id),
                "duplicate/empty workspace identity"
            );
            anyhow::ensure!(
                workspace.index > 0 && workspace.index <= 64,
                "invalid workspace position"
            );
            for window in &workspace.windows {
                anyhow::ensure!(
                    !window.id.is_empty() && ids.insert(&window.id),
                    "duplicate/empty window identity"
                );
                anyhow::ensure!(
                    window.recipe.len() <= 1,
                    "multiple launch recipes for a window"
                );
                for recipe in &window.recipe {
                    anyhow::ensure!(recipe.version == 1, "unsupported recipe version");
                    anyhow::ensure!(
                        !recipe.argv.is_empty() && !recipe.argv[0].is_empty(),
                        "empty launch command"
                    );
                    anyhow::ensure!(
                        std::path::Path::new(&recipe.cwd).is_absolute(),
                        "launch CWD must be absolute"
                    );
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
pub struct RestoreReport {
    pub state: String,
    pub restored: u32,
    pub reused: u32,
    pub issues: Vec<String>,
}
