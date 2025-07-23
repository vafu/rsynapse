use std::fmt::Debug;

/// The standardized data structure for a single result item.
/// This struct is sent over D-Bus. In a real application, you would
/// use zbus::zvariant::Type to ensure it's serializable.
/// For now, we will convert it to a simple String for simplicity.
#[derive(Debug)]
pub struct ResultItem {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub icon: Option<String>,
}

pub trait Plugin: Send + Sync {

    /// Returns the name of the plugin.
    fn name(&self) -> &'static str;

    /// Called by the daemon to get results for a given query.
    fn query(&self, query: &str) -> Vec<ResultItem>;
}
