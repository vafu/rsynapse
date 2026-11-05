use std::fmt::Debug;

/// The standardized data structure for a single result item.
#[derive(Debug, Clone, PartialEq)]
pub struct ResultItem {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub data: Option<String>,
    pub score: f64,
    /// Marks an already-applied/current value, such as the current project.
    pub current: bool,
}

/// A param-selectable surface exposed by a plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginGroup {
    /// Stable group id used in launcher params, for example `projects`.
    pub id: String,
    /// Human-readable group title.
    pub title: String,
    pub description: Option<String>,
    /// Additional accepted param spellings.
    pub aliases: Vec<String>,
}

/// One dynamic keyboard hint rendered by the launcher footer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    pub key: String,
    pub label: String,
}

/// Plugin-provided dynamic launcher header/footer content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginView {
    pub title: String,
    pub subtitle: Option<String>,
    pub placeholder: String,
    pub hints: Vec<Hint>,
}

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

/// Plugin-facing key event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Delete,
    Backspace,
    Tab,
    Enter,
    Escape,
    Other(String),
}

/// Outcome of forwarding a key event to a plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyOutcome {
    Ignored,
    Handled {
        refresh: bool,
        close: bool,
        message: Option<String>,
    },
}

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
    fn handle_key(&self, _event: &KeyEvent, _context: &QueryContext<'_>) -> KeyOutcome {
        KeyOutcome::Ignored
    }
}
