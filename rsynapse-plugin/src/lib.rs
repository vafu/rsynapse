use std::fmt::Debug;

/// The standardized data structure for a single result item.
/// This struct is sent over D-Bus. In a real application, you would
/// use zbus::zvariant::Type to ensure it's serializable.
/// For now, we will convert it to a simple String for simplicity.
#[derive(Debug, Clone)]
pub struct ResultItem {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub data: Option<String>,
    pub score: f64,
}

pub trait Plugin: Send + Sync {

    /// Returns the name of the plugin.
    fn name(&self) -> &'static str;

    /// Returns the default execute command template, if any.
    /// Placeholders: {id}, {title}, {description}, {icon}, {data}
    fn default_execute(&self) -> Option<&'static str> {
        None
    }

    /// Called by the daemon to get results for a given query.
    fn query(&self, query: &str) -> Vec<ResultItem>;
}
