use serde::{Deserialize, Serialize};

use crate::note::NoteId;

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriagePolicy {
    pub inbox_status: String,
    pub triaged_status: String,
    pub default_kind: String,
    pub kinds: Vec<String>,
}

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriageProposal {
    pub note_id: NoteId,
    pub proposed_kind: String,
    pub proposed_status: String,
    pub proposed_tags: Vec<String>,
    pub confidence: u8,
    pub reason: String,
}
