use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotebookHandle {
    pub object_path: String,
}

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotebookScope {
    pub path: String,
    pub tags: Vec<String>,
}

impl NotebookScope {
    pub fn unfiltered() -> Self {
        Self::default()
    }
}
