//! Shared public D-Bus contract for the Rsynapse launcher.
use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

/// Semantic launcher action. The UI owns the key binding and label.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
#[zvariant(signature = "s")]
pub enum ActionHint {
    #[default]
    Apply,
    Toggle,
    Remove,
    Focus,
}

/// Whether a plugin takes part in the aggregate search, direct invocation, or both.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
#[zvariant(signature = "s")]
pub enum PluginScope {
    #[default]
    DirectOnly,
    MainOnly,
    Both,
}

/// Stable plugin metadata. This is not launcher presentation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct LauncherPlugin {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub scope: PluginScope,
}

/// One launcher result row.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
pub struct LauncherResult {
    pub id: String,
    pub title: String,
    pub description: String,
    pub icon: String,
    pub data: String,
    pub current: bool,
    /// Keep the launcher open after activating this result.
    pub keep_open: bool,
    /// Plugin-provided relevance score; not shown by the UI.
    pub score: f64,
    /// Optional transient activity indicator for this row.
    #[serde(default)]
    pub activity: LauncherActivity,
}

/// Transient activity shown by the launcher row rather than encoded in its text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
#[zvariant(signature = "s")]
pub enum LauncherActivity {
    #[default]
    None,
    Connecting,
    Busy,
}

/// Search request with invocation params and the current query.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SearchRequest {
    pub params: Vec<String>,
    pub query: String,
    pub selected: String,
}

/// Search results plus the focused direct plugin's data-level context.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
pub struct SearchResponse {
    /// Directly invoked plugin id, or empty for aggregate search.
    pub plugin: String,
    /// Display name of the directly invoked plugin.
    pub plugin_name: String,
    /// Live one-line context supplied by the plugin, if any.
    pub summary: String,
    /// Semantic actions available for the current results.
    pub actions: Vec<ActionHint>,
    pub results: Vec<LauncherResult>,
}

/// Version of the static plugin trait/wire contract built into the daemon.
pub const PLUGIN_ABI_VERSION: u32 = 2;

/// Static launcher metadata: plugins and entry points.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
pub struct DescribeResponse {
    pub plugins: Vec<LauncherPlugin>,
    pub abi_version: u32,
}

/// One key event forwarded from the launcher UI to a plugin.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct KeyEvent {
    pub key: String,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// Key handler request scoped to the current launcher request and selection.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct KeyRequest {
    pub params: Vec<String>,
    pub query: String,
    pub selected: String,
    pub event: KeyEvent,
}

/// Result of forwarding a key event to a plugin.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct KeyResponse {
    pub handled: bool,
    pub refresh: bool,
    pub close: bool,
    pub message: String,
}

/// How the launcher reacts to the same open command while visible.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
#[zvariant(signature = "s")]
pub enum LauncherMode {
    /// Show on open, hide on the same command.
    #[default]
    Search,
    /// Show on open, advance the selection on the same command, and activate
    /// the highlighted item when the invoking modifier is released.
    Cycle,
}

/// Parameters and initial query used when showing the launcher.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ShowRequest {
    pub params: Vec<String>,
    pub query: String,
    pub mode: LauncherMode,
    /// Start on the second result instead of the first.
    pub preselect_next: bool,
}

#[zbus::proxy(
    interface = "org.rsynapse.Engine1",
    default_service = "org.rsynapse.Engine",
    default_path = "/org/rsynapse/Engine1"
)]
pub trait Engine {
    fn search(&self, request: SearchRequest) -> zbus::Result<SearchResponse>;
    fn describe(&self) -> zbus::Result<DescribeResponse>;
    fn handle_key(&self, request: KeyRequest) -> zbus::Result<KeyResponse>;
    fn execute(&self, id: &str) -> zbus::Result<String>;
}

#[zbus::proxy(
    interface = "org.rsynapse.UIToggle1",
    default_service = "org.rsynapse.UIToggle",
    default_path = "/org/rsynapse/UI"
)]
pub trait UiToggle {
    fn show(&self, request: ShowRequest) -> zbus::Result<()>;
    fn toggle(&self, request: ShowRequest) -> zbus::Result<()>;
    fn hide(&self) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_request_round_trips_through_json() {
        let request = SearchRequest {
            params: vec!["workspaces".into()],
            query: "remark".into(),
            selected: "project-1".into(),
        };
        let encoded = serde_json::to_string(&request).unwrap();
        let decoded: SearchRequest = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, request);
    }

    #[test]
    fn key_response_round_trips_through_json() {
        let response = KeyResponse {
            handled: true,
            refresh: true,
            close: false,
            message: "removed".into(),
        };
        let encoded = serde_json::to_string(&response).unwrap();
        let decoded: KeyResponse = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, response);
    }
}
