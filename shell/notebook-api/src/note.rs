use serde::{Deserialize, Serialize};
use std::fmt;

/// Canonical note identity, currently the `zk` notebook-relative path.
#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NoteId(String);

impl NoteId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NoteId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for NoteId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for NoteId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NoteKind {
    Fleeting,
    Task,
    Research,
    Idea,
    Decision,
    Permanent,
    Structure,
    Synthesized,
    SynthesisRule,
    Unknown,
}

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteMetadata {
    pub kind: NoteKind,
    pub synthesis_rule: String,
    pub synthesized_note: String,
}

impl Default for NoteMetadata {
    fn default() -> Self {
        Self {
            kind: NoteKind::Unknown,
            synthesis_rule: String::new(),
            synthesized_note: String::new(),
        }
    }
}

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Note {
    pub id: NoteId,
    pub title: String,
    pub path: String,
    pub link: String,
    pub tags: Vec<String>,
    pub checksum: String,
    pub created: String,
    pub modified: String,
    pub metadata: NoteMetadata,
}

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagCount {
    pub name: String,
    pub count: u32,
}

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NoteBody {
    pub note_id: NoteId,
    pub revision: NoteRevision,
    pub text: String,
    /// Frontmatter tags read from the same document revision as the body.
    #[serde(default)]
    pub tags: Vec<String>,
    pub blocks: Vec<NoteBlock>,
    pub spans: Vec<NoteSpan>,
}

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NoteRevision(String);

impl NoteRevision {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NoteRevision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextRange {
    pub char_start: u64,
    pub char_len: u64,
}

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteBlock {
    pub id: String,
    pub range: TextRange,
    pub kind: NoteBlockKind,
    pub heading_level: u8,
}

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteBlockKind {
    Paragraph,
    Heading,
    List,
    ListItem,
    Code,
    Quote,
}

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteSpan {
    pub range: TextRange,
    pub kind: NoteSpanKind,
}

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteSpanKind {
    Emphasis,
    Strong,
    Code,
    Link,
    TaskMarker,
}

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchResult {
    pub changed: bool,
    pub revision: NoteRevision,
}

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandResult {
    pub changed: bool,
    pub message: String,
}
