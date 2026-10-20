//! Only fields needed by the neutral API; native extensions are ignored.
use serde::Deserialize;
use workspace_dbus_api::Layout;

#[derive(Debug, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub(super) enum Event {
    Outputs(Vec<Output>),
    Workspaces(Vec<Workspace>),
    Windows(Vec<Window>),
    Layers(Vec<Layer>),
    Overview(Overview),
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
pub(super) struct Output {
    pub name: String,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    pub focused: bool,
    pub active_workspace: String,
}
fn enabled_by_default() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub(super) struct Workspace {
    pub id: String,
    #[serde(default)]
    pub layout: Layout,
    pub name: String,
    pub named: bool,
    pub index: u32,
    pub output: String,
    pub active: bool,
    pub focused: bool,
    pub urgent: bool,
    pub focused_window: String,
    #[serde(default)]
    pub window_count: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(super) struct Window {
    pub id: String,
    pub title: String,
    pub app_id: String,
    pub pid: i32,
    pub workspace: String,
    pub output: String,
    pub active: bool,
    pub floating: bool,
    pub urgent: bool,
    pub column: i64,
    pub row: i64,
}

#[derive(Debug, Deserialize)]
pub(super) struct Layer {
    pub output: String,
    pub mapped: bool,
    pub focused: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct Overview {
    pub open: bool,
}
