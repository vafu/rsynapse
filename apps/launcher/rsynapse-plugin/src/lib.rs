//! Plugin-facing traits built on the shared launcher wire contract.
use std::fmt::Debug;

pub use rsynapse_launcher_api::{
    ActionHint, KeyEvent, KeyResponse, LauncherActivity, LauncherPlugin,
    LauncherResult as ResultItem, PluginScope,
};

/// Static plugin metadata used for routing and configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PluginSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub scope: PluginScope,
}

impl PluginSpec {
    pub fn to_wire(self) -> LauncherPlugin {
        LauncherPlugin {
            id: self.id.to_owned(),
            name: self.name.to_owned(),
            aliases: self
                .aliases
                .iter()
                .map(|alias| (*alias).to_owned())
                .collect(),
            scope: self.scope,
        }
    }
}

/// Whether this query is part of aggregate search or a direct invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryMode {
    Main,
    Direct,
}

/// Request context passed to a plugin for search and key handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryContext<'a> {
    pub mode: QueryMode,
    /// Remaining invocation params after the direct entry point.
    pub params: &'a [String],
    pub query: &'a str,
    pub selected: Option<&'a str>,
}

/// Results plus data-level context for direct invocations.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PluginResponse {
    pub results: Vec<ResultItem>,
    /// Optional one-line live context for the launcher subtitle.
    pub summary: String,
    /// Semantic actions available for the current results.
    pub actions: Vec<ActionHint>,
}

/// Outcome of forwarding a key event to a plugin (wire-compatible).
pub type KeyOutcome = KeyResponse;

pub trait Plugin: Send + Sync {
    /// Stable routing and configuration metadata.
    fn spec(&self) -> PluginSpec;

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
