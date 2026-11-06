//! Plugin-facing traits built on the shared launcher wire contract.
use std::fmt::Debug;

pub use rsynapse_launcher_api::{
    KeyEvent, KeyResponse, LauncherGroup as PluginGroup, LauncherHint as Hint,
    LauncherResult as ResultItem, LauncherView as PluginView,
};

/// Request context passed to a plugin for search and key handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryContext<'a> {
    /// Focused group id when the first launcher param matched a group.
    pub group: Option<&'a str>,
    /// Remaining invocation params, or all params when no group matched.
    pub params: &'a [String],
    pub query: &'a str,
    pub selected: Option<&'a str>,
}

/// Results and optional dynamic view for one plugin query.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PluginResponse {
    pub view: Option<PluginView>,
    pub results: Vec<ResultItem>,
}

/// Outcome of forwarding a key event to a plugin (wire-compatible).
pub type KeyOutcome = KeyResponse;

pub trait Plugin: Send + Sync {
    /// Returns the name of the plugin.
    fn name(&self) -> &'static str;

    /// Returns the param-selectable groups exposed by this plugin.
    fn groups(&self) -> Vec<PluginGroup>;

    /// Returns the default execute command template, if any.
    /// Placeholders: {id}, {title}, {description}, {icon}, {data}
    fn default_execute(&self) -> Option<&'static str> {
        None
    }

    /// Performs a direct action for an item. Returning `None` falls back to the
    /// daemon's execute template.
    fn execute(&self, _item: &ResultItem) -> Option<Result<(), String>> {
        None
    }

    /// Called when the config file changes. Plugins can override this
    /// to reload their configuration.
    fn reload(&self) {}

    /// Called by the daemon to get results for a given request.
    fn query(&self, context: &QueryContext<'_>) -> PluginResponse;

    /// Called for keyboard events the launcher UI does not consume itself.
    fn handle_key(&self, _event: &KeyEvent, _context: &QueryContext<'_>) -> KeyResponse {
        KeyResponse::default()
    }
}
