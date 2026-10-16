//! A protocol-only producer: deliberately has no compositor dependency.

use std::sync::Arc;

use tokio::sync::RwLock;
use workspace_dbus_api::{output_path, window_path, workspace_path};
use zbus::{Connection, connection::Builder, interface, zvariant::OwnedObjectPath};

pub const OUTPUT: &str = "DP-1";
pub const WORKSPACE: &str = "DP-1:2/é";
pub const WINDOW: &str = "opaque-window/α:007";

pub struct State {
    pub connected: bool,
    pub generation: u64,
    pub window_present: bool,
    pub focused: bool,
    pub title: String,
    pub layout: String,
}

pub type SharedState = Arc<RwLock<State>>;

pub async fn serve() -> zbus::Result<(Connection, SharedState)> {
    let state = Arc::new(RwLock::new(State {
        connected: true,
        generation: 1,
        window_present: true,
        focused: true,
        title: "initial title".into(),
        layout: "scrolling".into(),
    }));
    let connection = Builder::session()?
        .serve_at(workspace_dbus_api::ROOT_PATH, zbus::fdo::ObjectManager)?
        .serve_at(workspace_dbus_api::ROOT_PATH, Manager(state.clone()))?
        .serve_at(output_path(OUTPUT), Output(state.clone()))?
        .serve_at(workspace_path(WORKSPACE), Workspace(state.clone()))?
        .serve_at(window_path(WINDOW), Window(state.clone()))?
        .name(workspace_dbus_api::BUS_NAME)?
        .build()
        .await?;
    Ok((connection, state))
}

pub struct Manager(pub SharedState);

#[interface(name = "org.rsynapse.Workspace.Manager1")]
impl Manager {
    #[zbus(property)]
    async fn connected(&self) -> bool {
        self.0.read().await.connected
    }

    #[zbus(property)]
    async fn generation(&self) -> u64 {
        self.0.read().await.generation
    }

    #[zbus(property)]
    async fn outputs(&self) -> Vec<OwnedObjectPath> {
        self.0
            .read()
            .await
            .connected
            .then(|| output_path(OUTPUT))
            .into_iter()
            .collect()
    }

    #[zbus(property)]
    async fn workspaces(&self) -> Vec<OwnedObjectPath> {
        self.0
            .read()
            .await
            .connected
            .then(|| workspace_path(WORKSPACE))
            .into_iter()
            .collect()
    }

    #[zbus(property)]
    async fn windows(&self) -> Vec<OwnedObjectPath> {
        let state = self.0.read().await;
        (state.connected && state.window_present)
            .then(|| window_path(WINDOW))
            .into_iter()
            .collect()
    }

    #[zbus(property)]
    async fn focused_output(&self) -> Vec<OwnedObjectPath> {
        self.0
            .read()
            .await
            .connected
            .then(|| output_path(OUTPUT))
            .into_iter()
            .collect()
    }

    #[zbus(property)]
    async fn focused_workspace(&self) -> Vec<OwnedObjectPath> {
        self.0
            .read()
            .await
            .connected
            .then(|| workspace_path(WORKSPACE))
            .into_iter()
            .collect()
    }

    #[zbus(property)]
    async fn focused_window(&self) -> Vec<OwnedObjectPath> {
        let state = self.0.read().await;
        (state.connected && state.window_present && state.focused)
            .then(|| window_path(WINDOW))
            .into_iter()
            .collect()
    }
}

pub struct Output(pub SharedState);

#[interface(name = "org.rsynapse.Workspace.Output1")]
impl Output {
    #[zbus(property(emits_changed_signal = "const"))]
    fn name(&self) -> &str {
        OUTPUT
    }

    #[zbus(property)]
    async fn focused(&self) -> bool {
        self.0.read().await.connected
    }

    #[zbus(property)]
    async fn current_workspace(&self) -> Vec<OwnedObjectPath> {
        vec![workspace_path(WORKSPACE)]
    }

    #[zbus(property)]
    async fn workspaces(&self) -> Vec<OwnedObjectPath> {
        vec![workspace_path(WORKSPACE)]
    }
}

pub struct Workspace(pub SharedState);

#[interface(name = "org.rsynapse.Workspace.Workspace1")]
impl Workspace {
    #[zbus(property)]
    async fn layout(&self) -> String {
        self.0.read().await.layout.clone()
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn id(&self) -> &str {
        WORKSPACE
    }

    #[zbus(property)]
    fn name(&self) -> Vec<String> {
        // Explicitly named "2" must not be confused with anonymous index 2.
        vec!["2".into()]
    }

    #[zbus(property)]
    fn index(&self) -> u32 {
        2
    }

    #[zbus(property)]
    fn output(&self) -> Vec<OwnedObjectPath> {
        vec![output_path(OUTPUT)]
    }

    #[zbus(property)]
    fn active(&self) -> bool {
        true
    }

    #[zbus(property)]
    async fn focused(&self) -> bool {
        self.0.read().await.connected
    }

    #[zbus(property)]
    fn urgent(&self) -> bool {
        false
    }

    #[zbus(property)]
    async fn active_window(&self) -> Vec<OwnedObjectPath> {
        self.windows().await
    }

    #[zbus(property)]
    async fn windows(&self) -> Vec<OwnedObjectPath> {
        self.0
            .read()
            .await
            .window_present
            .then(|| window_path(WINDOW))
            .into_iter()
            .collect()
    }
}

pub struct Window(pub SharedState);

#[interface(name = "org.rsynapse.Workspace.Window1")]
impl Window {
    #[zbus(property(emits_changed_signal = "const"))]
    fn id(&self) -> &str {
        WINDOW
    }

    #[zbus(property)]
    async fn title(&self) -> Vec<String> {
        vec![self.0.read().await.title.clone()]
    }

    #[zbus(property)]
    fn app_id(&self) -> Vec<String> {
        vec!["example.application".into()]
    }

    #[zbus(property)]
    fn pid(&self) -> Vec<i32> {
        Vec::new()
    }

    #[zbus(property)]
    fn workspace(&self) -> Vec<OwnedObjectPath> {
        vec![workspace_path(WORKSPACE)]
    }

    #[zbus(property)]
    fn output(&self) -> Vec<OwnedObjectPath> {
        vec![output_path(OUTPUT)]
    }

    #[zbus(property)]
    async fn focused(&self) -> bool {
        self.0.read().await.focused
    }

    #[zbus(property)]
    fn floating(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn urgent(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn column_index(&self) -> Vec<u64> {
        vec![0]
    }

    #[zbus(property)]
    fn row_index(&self) -> Vec<u64> {
        Vec::new()
    }
}
