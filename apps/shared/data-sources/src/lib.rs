//! Typed service adapters exposed as shell-reactive Observables.
//!
//! UI crates consume this crate; they must not construct service clients or
//! inspect D-Bus/process state directly. Reactive runtime primitives are
//! re-exported from `shell-reactive`.
use std::path::PathBuf;

pub mod audio;
pub mod backlight;
pub mod bluetooth;
pub mod bzbus;
pub mod dbus;
pub mod github;
pub mod icon_picker;
mod locus_watch;
pub mod mist;
pub mod mpris;
pub mod notifications;
pub mod power_profile;
pub mod proj;
pub mod session;
pub mod steward;
pub mod system_stats;
pub mod systray;
pub mod text;
pub mod wayland;
pub mod workspace;
pub mod workspace_launcher;
pub use workspace_dbus_api;
pub use workspace_policy;
pub mod workspace_icon;
pub mod workspace_name;
pub mod workspace_project;
pub mod write;

pub use locus_watch::{watch_relation, watch_relation_subject};
pub use shell_reactive::{
    ListChange, ListDiff, Observable, Source, SourceError, SourceErrors, SourceSubscription,
    StateSignal, combine_latest, combine_latest_vec, defer, diff, diff_by_key, diff_slices,
    diff_slices_by_key, error_count, errors, from_task, keyed_list, materialize_list, once, rx,
    rxrust, shared_by_key, switch_map, switch_map_list, switch_map_list_distinct,
};

pub use zbus::zvariant;

pub(crate) mod support {
    pub use shell_reactive::{from_stream_result, log_errors, record_source_error};
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SourceSnapshot {
    pub path: PathBuf,
}
