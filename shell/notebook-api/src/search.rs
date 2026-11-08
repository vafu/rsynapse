use serde::{Deserialize, Serialize};

use crate::note::Note;

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LexicalSearchQuery {
    pub text: String,
    pub required_tags: Vec<String>,
    pub limit: u64,
    pub snippet_chars: u64,
}

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResult {
    pub note: Note,
    pub rank: u64,
    pub mode: String,
    pub snippet: String,
}

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchPage {
    pub results: Vec<SearchResult>,
    pub limit: u64,
    pub truncated: bool,
}
