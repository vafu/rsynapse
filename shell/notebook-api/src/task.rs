use serde::{Deserialize, Serialize};

use crate::note::{NoteId, NoteRevision};

/// Completion state represented by a Markdown checkbox marker.
#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Open,
    Completed,
}

/// A revision-bound source location for one Markdown task item.
#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskLocator {
    pub note_id: NoteId,
    pub note_revision: NoteRevision,
    pub line: u64,
}

/// A task projected from a Markdown checkbox in a source note.
#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskItem {
    pub locator: TaskLocator,
    pub parent: Vec<TaskLocator>,
    pub detail_note_id: Vec<NoteId>,
    pub note_title: String,
    pub text: String,
    pub status: TaskStatus,
}
