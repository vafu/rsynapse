use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceStatus {
    pub configured: bool,
    pub notebook_dir: String,
    pub zk_ok: bool,
    pub scope_count: u64,
    pub note_count: u64,
    pub last_error: String,
}
