//! Projection host. Consumers depend on workspace-dbus-api, never this crate.
mod backend;
mod dbus;
mod model;
mod service;

pub use backend::BackendKind;
pub use service::{Config, run};
