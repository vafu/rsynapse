//! Optional, epoch-scoped compositor controls. Bind the proxy to a unique bus
//! owner and pass that owner's observed generation on every mutation.
use crate::{Id, Layout};
use zbus::proxy;

#[proxy(
    interface = "org.rsynapse.Workspace.Control1",
    default_service = "org.rsynapse.Workspace",
    default_path = "/org/rsynapse/Workspace",
    gen_blocking = false
)]
pub trait Control {
    #[zbus(property)]
    fn capabilities(&self) -> zbus::Result<Vec<String>>;
    async fn focus_window(&self, generation: u64, window: &str) -> zbus::Result<()>;
    /// Select a one-based position; dynamic inventories may return their last
    /// empty workspace instead. Returns the actual live identity, never a slot.
    async fn select_workspace(&self, generation: u64, output: &str, index: u32)
    -> zbus::Result<Id>;
    async fn set_layout(
        &self,
        generation: u64,
        workspace: &str,
        layout: Layout,
    ) -> zbus::Result<()>;
    async fn place_window(
        &self,
        generation: u64,
        window: &str,
        workspace: &str,
        floating: bool,
    ) -> zbus::Result<()>;
}
