use zbus::{proxy, zvariant::OwnedObjectPath};

use crate::{Id, Layout};

/// Live compositor projection. All optional object references contain 0 or 1 paths.
#[proxy(
    interface = "org.rsynapse.Workspace.Manager1",
    default_service = "org.rsynapse.Workspace",
    default_path = "/org/rsynapse/Workspace",
    gen_blocking = false
)]
pub trait Manager {
    /// True only after the initial inventory is ready for observation.
    #[zbus(property)]
    fn connected(&self) -> zbus::Result<bool>;

    /// Changes when the projection is reset, not on ordinary state changes.
    #[zbus(property)]
    fn generation(&self) -> zbus::Result<u64>;

    #[zbus(property)]
    fn outputs(&self) -> zbus::Result<Vec<OwnedObjectPath>>;

    #[zbus(property)]
    fn workspaces(&self) -> zbus::Result<Vec<OwnedObjectPath>>;

    #[zbus(property)]
    fn windows(&self) -> zbus::Result<Vec<OwnedObjectPath>>;

    #[zbus(property)]
    fn focused_output(&self) -> zbus::Result<Vec<OwnedObjectPath>>;

    #[zbus(property)]
    fn focused_workspace(&self) -> zbus::Result<Vec<OwnedObjectPath>>;

    /// The window holding application keyboard focus, or an empty array.
    #[zbus(property)]
    fn focused_window(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[proxy(
    interface = "org.rsynapse.Workspace.Output1",
    default_service = "org.rsynapse.Workspace",
    gen_blocking = false
)]
pub trait Output {
    #[zbus(property(emits_changed_signal = "const"))]
    fn name(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn focused(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn current_workspace(&self) -> zbus::Result<Vec<OwnedObjectPath>>;

    #[zbus(property)]
    fn workspaces(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[proxy(
    interface = "org.rsynapse.Workspace.Workspace1",
    default_service = "org.rsynapse.Workspace",
    gen_blocking = false
)]
pub trait Workspace {
    #[zbus(property(emits_changed_signal = "const"))]
    fn id(&self) -> zbus::Result<Id>;

    /// Effective current layout; changes independently of workspace selection.
    #[zbus(property)]
    fn layout(&self) -> zbus::Result<Layout>;

    /// An explicit name, or an empty array for an anonymous workspace.
    #[zbus(property)]
    fn name(&self) -> zbus::Result<Vec<String>>;

    /// One-based display position on the output, independent of identity.
    #[zbus(property)]
    fn index(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn output(&self) -> zbus::Result<Vec<OwnedObjectPath>>;

    /// Whether this workspace is current on its output, not global focus.
    #[zbus(property)]
    fn active(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn focused(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn urgent(&self) -> zbus::Result<bool>;

    /// Remembered application focus on this workspace, even while inactive.
    #[zbus(property)]
    fn active_window(&self) -> zbus::Result<Vec<OwnedObjectPath>>;

    #[zbus(property)]
    fn windows(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[proxy(
    interface = "org.rsynapse.Workspace.Window1",
    default_service = "org.rsynapse.Workspace",
    gen_blocking = false
)]
pub trait Window {
    #[zbus(property(emits_changed_signal = "const"))]
    fn id(&self) -> zbus::Result<Id>;

    #[zbus(property)]
    fn title(&self) -> zbus::Result<Vec<String>>;

    #[zbus(property)]
    fn app_id(&self) -> zbus::Result<Vec<String>>;

    /// Positive process ID when known; unknown is an empty array.
    #[zbus(property)]
    fn pid(&self) -> zbus::Result<Vec<i32>>;

    #[zbus(property)]
    fn workspace(&self) -> zbus::Result<Vec<OwnedObjectPath>>;

    /// Output is independent of workspace membership (e.g. a scratchpad window).
    #[zbus(property)]
    fn output(&self) -> zbus::Result<Vec<OwnedObjectPath>>;

    /// Global application keyboard focus, not remembered workspace-local focus.
    #[zbus(property)]
    fn focused(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn floating(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn urgent(&self) -> zbus::Result<bool>;

    /// Zero-based scrolling position; absent where the layout has no columns.
    #[zbus(property)]
    fn column_index(&self) -> zbus::Result<Vec<u64>>;

    /// Zero-based scrolling position; absent where the layout has no rows.
    #[zbus(property)]
    fn row_index(&self) -> zbus::Result<Vec<u64>>;
}
