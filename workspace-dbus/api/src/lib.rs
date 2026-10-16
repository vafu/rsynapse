//! Compositor-independent D-Bus API for outputs, workspaces, windows and focus.
//!
//! This crate owns the wire contract and typed client proxies. It contains no
//! compositor protocol, UI framework, project-management or relation-store code.
//! See the workspace's CONTRACT.md for identity, focus and lifecycle semantics.

mod layout;
pub mod paths;
mod proxies;

pub use layout::Layout;
pub use paths::{output_path, window_path, workspace_path};
pub use proxies::{ManagerProxy, OutputProxy, WindowProxy, WorkspaceProxy};

/// Well-known name on the session bus.
pub const BUS_NAME: &str = "org.rsynapse.Workspace";
/// Manager object and standard ObjectManager root.
pub const ROOT_PATH: &str = "/org/rsynapse/Workspace";
pub const MANAGER_INTERFACE: &str = "org.rsynapse.Workspace.Manager1";
pub const OUTPUT_INTERFACE: &str = "org.rsynapse.Workspace.Output1";
pub const WORKSPACE_INTERFACE: &str = "org.rsynapse.Workspace.Workspace1";
pub const WINDOW_INTERFACE: &str = "org.rsynapse.Workspace.Window1";

/// An opaque, nonempty string identifying a live workspace or window.
///
/// IDs are not numeric indices, object paths, persistent project identities, or
/// a format that clients may parse. Their validity is scoped to the current
/// service owner and manager generation. Optional references travel as object
/// paths, not sentinel IDs.
pub type Id = String;
