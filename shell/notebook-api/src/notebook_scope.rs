use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

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

    pub fn normalized_tags(&self) -> Vec<String> {
        self.tags
            .iter()
            .map(|tag| tag.trim())
            .filter(|tag| !tag.is_empty())
            .map(ToString::to_string)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn storage_path(&self) -> Option<&str> {
        let path = self.path.trim();
        (!path.is_empty()).then_some(path)
    }
}
