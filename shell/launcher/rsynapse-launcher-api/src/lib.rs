//! Shared public D-Bus contract for the Rsynapse launcher.
use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

/// One dynamic keyboard hint shown by the launcher footer.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct LauncherHint {
    pub key: String,
    pub label: String,
}

/// Launcher header and footer content for the current request/group.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct LauncherView {
    pub title: String,
    pub subtitle: String,
    pub placeholder: String,
    pub hints: Vec<LauncherHint>,
}

/// A param-selectable plugin group.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct LauncherGroup {
    pub id: String,
    pub title: String,
    pub description: String,
    pub aliases: Vec<String>,
    pub plugin: String,
}

/// One launcher result row.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct LauncherResult {
    pub id: String,
    pub title: String,
    pub description: String,
    pub icon: String,
    pub data: String,
    pub current: bool,
}

/// Search request with invocation params and the current query.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SearchRequest {
    pub params: Vec<String>,
    pub query: String,
    pub selected: String,
}

/// Search results plus the dynamic view for the resolved group.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SearchResponse {
    pub view: LauncherView,
    pub results: Vec<LauncherResult>,
}

/// Static launcher metadata: default view and all available groups.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DescribeResponse {
    pub default_view: LauncherView,
    pub groups: Vec<LauncherGroup>,
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

/// Parameters and initial query used when showing the launcher.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ShowRequest {
    pub params: Vec<String>,
    pub query: String,
    /// Keep the launcher open while the invoking modifier is held and activate
    /// the selection when that modifier is released.
    pub hold: bool,
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
