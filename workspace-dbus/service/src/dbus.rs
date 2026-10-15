use crate::model::Projection;
use std::sync::Arc;
use tokio::sync::RwLock;
use zbus::{interface, zvariant::OwnedObjectPath};
pub type State = Arc<RwLock<Projection>>;

pub struct Manager(pub State);
#[interface(name = "org.rsynapse.Workspace.Manager1")]
impl Manager {
    #[zbus(property)]
    async fn connected(&self) -> bool {
        self.0.read().await.manager.connected
    }
    #[zbus(property)]
    async fn generation(&self) -> u64 {
        self.0.read().await.manager.generation
    }
    #[zbus(property)]
    async fn outputs(&self) -> Vec<OwnedObjectPath> {
        self.0.read().await.manager.outputs.clone()
    }
    #[zbus(property)]
    async fn workspaces(&self) -> Vec<OwnedObjectPath> {
        self.0.read().await.manager.workspaces.clone()
    }
    #[zbus(property)]
    async fn windows(&self) -> Vec<OwnedObjectPath> {
        self.0.read().await.manager.windows.clone()
    }
    #[zbus(property)]
    async fn focused_output(&self) -> Vec<OwnedObjectPath> {
        self.0.read().await.manager.focused_output.clone()
    }
    #[zbus(property)]
    async fn focused_workspace(&self) -> Vec<OwnedObjectPath> {
        self.0.read().await.manager.focused_workspace.clone()
    }
    #[zbus(property)]
    async fn focused_window(&self) -> Vec<OwnedObjectPath> {
        self.0.read().await.manager.focused_window.clone()
    }
}

pub struct Output {
    pub state: State,
    pub id: String,
}
#[interface(name = "org.rsynapse.Workspace.Output1")]
impl Output {
    #[zbus(property(emits_changed_signal = "const"))]
    fn name(&self) -> &str {
        &self.id
    }
    #[zbus(property)]
    async fn focused(&self) -> bool {
        self.state
            .read()
            .await
            .outputs
            .get(&self.id)
            .is_some_and(|v| v.focused)
    }
    #[zbus(property)]
    async fn current_workspace(&self) -> Vec<OwnedObjectPath> {
        self.state
            .read()
            .await
            .outputs
            .get(&self.id)
            .map(|v| v.current_workspace.clone())
            .unwrap_or_default()
    }
    #[zbus(property)]
    async fn workspaces(&self) -> Vec<OwnedObjectPath> {
        self.state
            .read()
            .await
            .outputs
            .get(&self.id)
            .map(|v| v.workspaces.clone())
            .unwrap_or_default()
    }
}

pub struct Workspace {
    pub state: State,
    pub id: String,
}
#[interface(name = "org.rsynapse.Workspace.Workspace1")]
impl Workspace {
    #[zbus(property(emits_changed_signal = "const"))]
    fn id(&self) -> &str {
        &self.id
    }
    #[zbus(property)]
    async fn name(&self) -> Vec<String> {
        self.state
            .read()
            .await
            .workspaces
            .get(&self.id)
            .map(|v| v.name.clone())
            .unwrap_or_default()
    }
    #[zbus(property)]
    async fn index(&self) -> u32 {
        self.state
            .read()
            .await
            .workspaces
            .get(&self.id)
            .map(|v| v.index)
            .unwrap_or_default()
    }
    #[zbus(property)]
    async fn output(&self) -> Vec<OwnedObjectPath> {
        self.state
            .read()
            .await
            .workspaces
            .get(&self.id)
            .map(|v| v.output.clone())
            .unwrap_or_default()
    }
    #[zbus(property)]
    async fn active(&self) -> bool {
        self.state
            .read()
            .await
            .workspaces
            .get(&self.id)
            .is_some_and(|v| v.active)
    }
    #[zbus(property)]
    async fn focused(&self) -> bool {
        self.state
            .read()
            .await
            .workspaces
            .get(&self.id)
            .is_some_and(|v| v.focused)
    }
    #[zbus(property)]
    async fn urgent(&self) -> bool {
        self.state
            .read()
            .await
            .workspaces
            .get(&self.id)
            .is_some_and(|v| v.urgent)
    }
    #[zbus(property)]
    async fn active_window(&self) -> Vec<OwnedObjectPath> {
        self.state
            .read()
            .await
            .workspaces
            .get(&self.id)
            .map(|v| v.active_window.clone())
            .unwrap_or_default()
    }
    #[zbus(property)]
    async fn windows(&self) -> Vec<OwnedObjectPath> {
        self.state
            .read()
            .await
            .workspaces
            .get(&self.id)
            .map(|v| v.windows.clone())
            .unwrap_or_default()
    }
}

pub struct Window {
    pub state: State,
    pub id: String,
}
#[interface(name = "org.rsynapse.Workspace.Window1")]
impl Window {
    #[zbus(property(emits_changed_signal = "const"))]
    fn id(&self) -> &str {
        &self.id
    }
    #[zbus(property)]
    async fn title(&self) -> Vec<String> {
        self.state
            .read()
            .await
            .windows
            .get(&self.id)
            .map(|v| v.title.clone())
            .unwrap_or_default()
    }
    #[zbus(property)]
    async fn app_id(&self) -> Vec<String> {
        self.state
            .read()
            .await
            .windows
            .get(&self.id)
            .map(|v| v.app_id.clone())
            .unwrap_or_default()
    }
    #[zbus(property)]
    async fn pid(&self) -> Vec<i32> {
        self.state
            .read()
            .await
            .windows
            .get(&self.id)
            .map(|v| v.pid.clone())
            .unwrap_or_default()
    }
    #[zbus(property)]
    async fn workspace(&self) -> Vec<OwnedObjectPath> {
        self.state
            .read()
            .await
            .windows
            .get(&self.id)
            .map(|v| v.workspace.clone())
            .unwrap_or_default()
    }
    #[zbus(property)]
    async fn output(&self) -> Vec<OwnedObjectPath> {
        self.state
            .read()
            .await
            .windows
            .get(&self.id)
            .map(|v| v.output.clone())
            .unwrap_or_default()
    }
    #[zbus(property)]
    async fn focused(&self) -> bool {
        self.state
            .read()
            .await
            .windows
            .get(&self.id)
            .is_some_and(|v| v.focused)
    }
    #[zbus(property)]
    async fn floating(&self) -> bool {
        self.state
            .read()
            .await
            .windows
            .get(&self.id)
            .is_some_and(|v| v.floating)
    }
    #[zbus(property)]
    async fn urgent(&self) -> bool {
        self.state
            .read()
            .await
            .windows
            .get(&self.id)
            .is_some_and(|v| v.urgent)
    }
    #[zbus(property)]
    async fn column_index(&self) -> Vec<u64> {
        self.state
            .read()
            .await
            .windows
            .get(&self.id)
            .map(|v| v.column_index.clone())
            .unwrap_or_default()
    }
    #[zbus(property)]
    async fn row_index(&self) -> Vec<u64> {
        self.state
            .read()
            .await
            .windows
            .get(&self.id)
            .map(|v| v.row_index.clone())
            .unwrap_or_default()
    }
}
