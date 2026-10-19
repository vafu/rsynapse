//! Optional controls. Observation runs independently so action acknowledgments
//! and projection signals cannot deadlock one another.
use crate::{
    backend::{BackendKind, umbriel},
    dbus::State,
    model::Projection,
};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{Mutex, watch};
use workspace_dbus_api::{Layout, workspace_path};
use zbus::{fdo, interface};

pub(crate) struct Control {
    pub state: State,
    pub kind: BackendKind,
    pub socket: Option<PathBuf>,
    pub changed: watch::Receiver<u64>,
    pub lock: Arc<Mutex<()>>,
}

fn validate(snapshot: &Projection, generation: u64) -> anyhow::Result<()> {
    anyhow::ensure!(
        snapshot.manager.connected && snapshot.manager.generation == generation,
        "compositor source epoch changed"
    );
    Ok(())
}

impl Control {
    async fn snapshot(&self, generation: u64) -> anyhow::Result<Projection> {
        let snapshot = self.state.read().await.clone();
        validate(&snapshot, generation)?;
        Ok(snapshot)
    }

    async fn wait<T>(
        &self,
        generation: u64,
        predicate: impl Fn(&Projection) -> Option<T>,
    ) -> anyhow::Result<T> {
        let mut changes = self.changed.clone();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(value) = predicate(&self.snapshot(generation).await?) {
                    return Ok(value);
                }
                changes.changed().await?;
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("compositor did not acknowledge placement"))?
    }

    async fn action(&self, generation: u64, action: String) -> anyhow::Result<()> {
        self.snapshot(generation).await?;
        anyhow::ensure!(
            self.kind == BackendKind::Umbriel,
            "backend does not support session controls"
        );
        tokio::time::timeout(
            Duration::from_secs(5),
            umbriel::command(self.socket.as_deref(), &action),
        )
        .await??;
        self.snapshot(generation).await?;
        Ok(())
    }

    async fn select(&self, generation: u64, output: &str, index: u32) -> anyhow::Result<String> {
        anyhow::ensure!(index > 0 && index <= 64, "workspace position must be 1..64");
        let snapshot = self.snapshot(generation).await?;
        let target = snapshot
            .workspaces
            .iter()
            .filter(|(_, w)| w.output == vec![workspace_dbus_api::output_path(output)])
            .filter(|(_, w)| w.index <= index)
            .max_by_key(|(_, w)| w.index)
            .map(|(id, _)| id.clone())
            .ok_or_else(|| anyhow::anyhow!("output has no available workspace"))?;
        let position = snapshot.workspaces[&target].index;
        self.action(generation, format!("workspace-switch:{position}/{output}"))
            .await?;
        self.wait(generation, |s| {
            s.outputs
                .get(output)
                .filter(|o| o.current_workspace == vec![workspace_path(&target)])
                .map(|_| target.clone())
        })
        .await
    }

    async fn focus_workspace(&self, generation: u64, workspace: &str) -> anyhow::Result<()> {
        let snapshot = self.snapshot(generation).await?;
        let target = snapshot
            .workspaces
            .get(workspace)
            .ok_or_else(|| anyhow::anyhow!("workspace no longer exists"))?;
        let output = snapshot
            .outputs
            .keys()
            .find(|id| target.output == vec![workspace_dbus_api::output_path(id)])
            .ok_or_else(|| anyhow::anyhow!("workspace has no output"))?;
        let selected = self.select(generation, output, target.index).await?;
        anyhow::ensure!(selected == workspace, "workspace changed during selection");
        Ok(())
    }
}

fn failed(error: anyhow::Error) -> fdo::Error {
    fdo::Error::Failed(format!("{error:#}"))
}

#[interface(name = "org.rsynapse.Workspace.Control1")]
impl Control {
    #[zbus(property)]
    fn capabilities(&self) -> Vec<String> {
        if self.kind == BackendKind::Umbriel {
            ["select-workspace", "set-layout", "place-window"]
                .map(str::to_owned)
                .to_vec()
        } else {
            vec![]
        }
    }

    async fn select_workspace(
        &self,
        generation: u64,
        output: String,
        index: u32,
    ) -> fdo::Result<String> {
        let _guard = self.lock.lock().await;
        self.select(generation, &output, index)
            .await
            .map_err(failed)
    }

    async fn set_layout(
        &self,
        generation: u64,
        workspace: String,
        layout: Layout,
    ) -> fdo::Result<()> {
        let _guard = self.lock.lock().await;
        let result = async {
            anyhow::ensure!(
                layout != Layout::Unknown,
                "unknown layout cannot be restored"
            );
            self.focus_workspace(generation, &workspace).await?;
            anyhow::ensure!(
                self.snapshot(generation).await?.manager.focused_workspace
                    == vec![workspace_path(&workspace)],
                "workspace focus changed during layout selection"
            );
            let name = match layout {
                Layout::Scrolling => "scrolling",
                Layout::Master => "master",
                Layout::Dwindle => "dwindle",
                _ => unreachable!(),
            };
            self.action(generation, format!("workspace-set-layout:{name}"))
                .await?;
            self.wait(generation, |s| {
                s.workspaces
                    .get(&workspace)
                    .filter(|w| w.layout == layout)
                    .map(|_| ())
            })
            .await
        }
        .await;
        result.map_err(failed)
    }

    async fn place_window(
        &self,
        generation: u64,
        window: String,
        workspace: String,
        floating: bool,
    ) -> fdo::Result<()> {
        let _guard = self.lock.lock().await;
        let result = async {
            let snapshot = self.snapshot(generation).await?;
            let current = snapshot
                .windows
                .get(&window)
                .ok_or_else(|| anyhow::anyhow!("window no longer exists"))?;
            if current.workspace != vec![workspace_path(&workspace)] {
                self.action(generation, format!("window-focus:{window}"))
                    .await?;
                self.wait(generation, |s| {
                    (s.manager.focused_window == vec![workspace_dbus_api::window_path(&window)])
                        .then_some(())
                })
                .await?;
                let snapshot = self.snapshot(generation).await?;
                let target = snapshot
                    .workspaces
                    .get(&workspace)
                    .ok_or_else(|| anyhow::anyhow!("destination workspace expired"))?;
                anyhow::ensure!(
                    snapshot.manager.focused_window
                        == vec![workspace_dbus_api::window_path(&window)],
                    "window focus changed during placement"
                );
                let output = snapshot
                    .outputs
                    .keys()
                    .find(|id| target.output == vec![workspace_dbus_api::output_path(id)])
                    .ok_or_else(|| anyhow::anyhow!("destination has no output"))?;
                self.action(
                    generation,
                    format!("window-move-to-workspace:{}/{output}", target.index),
                )
                .await?;
                self.wait(generation, |s| {
                    s.windows
                        .get(&window)
                        .filter(|w| w.workspace == vec![workspace_path(&workspace)])
                        .map(|_| ())
                })
                .await?;
            }
            let snapshot = self.snapshot(generation).await?;
            if snapshot
                .windows
                .get(&window)
                .is_some_and(|w| w.floating != floating)
            {
                self.action(generation, format!("window-toggle-floating:{window}"))
                    .await?;
                self.wait(generation, |s| {
                    s.windows
                        .get(&window)
                        .filter(|w| w.floating == floating)
                        .map(|_| ())
                })
                .await?;
            }
            Ok(())
        }
        .await;
        result.map_err(failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn controls_reject_disconnected_and_replaced_epochs() {
        let mut snapshot = Projection::default();
        assert!(validate(&snapshot, 0).is_err());
        snapshot.manager.connected = true;
        snapshot.manager.generation = 7;
        assert!(validate(&snapshot, 6).is_err());
        assert!(validate(&snapshot, 7).is_ok());
    }
}
