//! Vendored Remarked notebook contract used by the shell task/notes surface.
//!
//! Kept deliberately small: only the D-Bus wire types the shell reads and
//! writes. The Remarked service remains authoritative; this crate pins the
//! client-side contract so the shell builds from a clean checkout.

pub mod note;
pub mod notebook_scope;
pub mod search;
pub mod task;
pub mod triage;
pub mod workspace;

use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "dbus", derive(zbus::zvariant::Type))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub api_schema_version: u16,
    pub cli_schema_version: u16,
    pub names: Vec<String>,
}
