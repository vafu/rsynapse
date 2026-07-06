use std::{collections::HashSet, sync::Arc, time::Duration};

use tokio::{sync::RwLock, time::sleep};
use tracing::{debug, info, warn};
use zbus::{Connection, connection::Builder};

use niri_dbus::paths;

use crate::{
    dbus::{OutputInterface, RootInterface, SharedState, WindowInterface, WorkspaceInterface},
    ipc,
    state::{NiriState, ObjectDelta},
};

pub async fn run() -> anyhow::Result<()> {
    let state: SharedState = Arc::new(RwLock::new(NiriState::default()));
    let connection = Builder::session()?
        .serve_at(paths::ROOT_PATH, RootInterface::new(state.clone()))?
        .serve_at(paths::ROOT_PATH, zbus::fdo::ObjectManager)?
        .name(paths::BUS_NAME)?
        .build()
        .await?;

    info!("owning {} at {}", paths::BUS_NAME, paths::ROOT_PATH);
    let service = Service {
        connection,
        state,
        registered_outputs: HashSet::new(),
        registered_workspaces: HashSet::new(),
        registered_windows: HashSet::new(),
    };

    tokio::select! {
        result = service.run_niri_loop() => result,
        result = tokio::signal::ctrl_c() => {
            result?;
            Ok(())
        }
    }
}

struct Service {
    connection: Connection,
    state: SharedState,
    registered_outputs: HashSet<String>,
    registered_workspaces: HashSet<u64>,
    registered_windows: HashSet<u64>,
}

impl Service {
    async fn run_niri_loop(mut self) -> anyhow::Result<()> {
        loop {
            match self.run_connected_once().await {
                Ok(()) => warn!("niri event stream ended"),
                Err(error) => warn!("niri connection failed: {error:#}"),
            }

            let delta = self.state.write().await.mark_disconnected();
            self.apply_object_delta(delta).await?;
            self.emit_root_changes().await;
            sleep(Duration::from_secs(1)).await;
        }
    }

    async fn run_connected_once(&mut self) -> anyhow::Result<()> {
        let (version, outputs) = ipc::initial_snapshot().await?;
        let delta = self.state.write().await.mark_connected(version, outputs);
        self.apply_object_delta(delta).await?;
        self.emit_root_changes().await;
        self.emit_object_changes().await;

        let mut stream = ipc::event_stream().await?;
        loop {
            let event = stream.read_event().await?;
            debug!(?event, "niri event");
            let delta = self.state.write().await.apply_event(event)?;
            self.apply_object_delta(delta).await?;
            self.emit_root_changes().await;
            self.emit_object_changes().await;
        }
    }

    async fn apply_object_delta(&mut self, delta: ObjectDelta) -> anyhow::Result<()> {
        for window in sorted(delta.removed.windows) {
            if self.registered_windows.contains(&window) {
                self.connection
                    .object_server()
                    .remove::<WindowInterface, _>(paths::window_path(window))
                    .await?;
                self.registered_windows.remove(&window);
            }
        }
        for workspace in sorted(delta.removed.workspaces) {
            if self.registered_workspaces.contains(&workspace) {
                self.connection
                    .object_server()
                    .remove::<WorkspaceInterface, _>(paths::workspace_path(workspace))
                    .await?;
                self.registered_workspaces.remove(&workspace);
            }
        }
        for output in sorted_strings(delta.removed.outputs) {
            if self.registered_outputs.contains(&output) {
                self.connection
                    .object_server()
                    .remove::<OutputInterface, _>(paths::output_path(&output))
                    .await?;
                self.registered_outputs.remove(&output);
            }
        }

        for output in sorted_strings(delta.added.outputs) {
            if !self.registered_outputs.contains(&output) {
                self.connection
                    .object_server()
                    .at(
                        paths::output_path(&output),
                        OutputInterface::new(self.state.clone(), output.clone()),
                    )
                    .await?;
                self.registered_outputs.insert(output);
            }
        }
        for workspace in sorted(delta.added.workspaces) {
            if !self.registered_workspaces.contains(&workspace) {
                self.connection
                    .object_server()
                    .at(
                        paths::workspace_path(workspace),
                        WorkspaceInterface::new(self.state.clone(), workspace),
                    )
                    .await?;
                self.registered_workspaces.insert(workspace);
            }
        }
        for window in sorted(delta.added.windows) {
            if !self.registered_windows.contains(&window) {
                self.connection
                    .object_server()
                    .at(
                        paths::window_path(window),
                        WindowInterface::new(self.state.clone(), window),
                    )
                    .await?;
                self.registered_windows.insert(window);
            }
        }

        Ok(())
    }

    async fn emit_root_changes(&self) {
        let Ok(iface_ref) = self
            .connection
            .object_server()
            .interface::<_, RootInterface>(paths::ROOT_PATH)
            .await
        else {
            return;
        };
        let iface = iface_ref.get().await;
        let context = iface_ref.signal_context();
        let _ = iface.connected_changed(context).await;
        let _ = iface.compositor_version_changed(context).await;
        let _ = iface.generation_changed(context).await;
        let _ = iface.outputs_changed(context).await;
        let _ = iface.workspaces_changed(context).await;
        let _ = iface.windows_changed(context).await;
        let _ = iface.focused_output_changed(context).await;
        let _ = iface.focused_workspace_changed(context).await;
        let _ = iface.focused_window_changed(context).await;
        let _ = iface.keyboard_layouts_changed(context).await;
        let _ = iface.keyboard_layout_index_changed(context).await;
        let _ = iface.overview_open_changed(context).await;
        let _ = iface.config_load_failed_changed(context).await;
    }

    async fn emit_object_changes(&self) {
        for output in sorted_strings(self.registered_outputs.clone()) {
            self.emit_output_changes(&output).await;
        }
        for workspace in sorted(self.registered_workspaces.clone()) {
            self.emit_workspace_changes(workspace).await;
        }
        for window in sorted(self.registered_windows.clone()) {
            self.emit_window_changes(window).await;
        }
    }

    async fn emit_output_changes(&self, output: &str) {
        let path = paths::output_path(output);
        let Ok(iface_ref) = self
            .connection
            .object_server()
            .interface::<_, OutputInterface>(path)
            .await
        else {
            return;
        };
        let iface = iface_ref.get().await;
        let context = iface_ref.signal_context();
        let _ = iface.make_changed(context).await;
        let _ = iface.model_changed(context).await;
        let _ = iface.serial_changed(context).await;
        let _ = iface.focused_changed(context).await;
        let _ = iface.current_workspace_changed(context).await;
        let _ = iface.workspaces_changed(context).await;
        let _ = iface.physical_width_mm_changed(context).await;
        let _ = iface.physical_height_mm_changed(context).await;
        let _ = iface.current_mode_width_changed(context).await;
        let _ = iface.current_mode_height_changed(context).await;
        let _ = iface.current_mode_refresh_mhz_changed(context).await;
        let _ = iface.current_mode_preferred_changed(context).await;
        let _ = iface.custom_mode_changed(context).await;
        let _ = iface.logical_x_changed(context).await;
        let _ = iface.logical_y_changed(context).await;
        let _ = iface.logical_width_changed(context).await;
        let _ = iface.logical_height_changed(context).await;
        let _ = iface.scale_changed(context).await;
        let _ = iface.transform_changed(context).await;
        let _ = iface.vrr_supported_changed(context).await;
        let _ = iface.vrr_enabled_changed(context).await;
    }

    async fn emit_workspace_changes(&self, workspace: u64) {
        let path = paths::workspace_path(workspace);
        let Ok(iface_ref) = self
            .connection
            .object_server()
            .interface::<_, WorkspaceInterface>(path)
            .await
        else {
            return;
        };
        let iface = iface_ref.get().await;
        let context = iface_ref.signal_context();
        let _ = iface.name_changed(context).await;
        let _ = iface.index_changed(context).await;
        let _ = iface.output_changed(context).await;
        let _ = iface.active_changed(context).await;
        let _ = iface.focused_changed(context).await;
        let _ = iface.urgent_changed(context).await;
        let _ = iface.active_window_changed(context).await;
        let _ = iface.windows_changed(context).await;
    }

    async fn emit_window_changes(&self, window: u64) {
        let path = paths::window_path(window);
        let Ok(iface_ref) = self
            .connection
            .object_server()
            .interface::<_, WindowInterface>(path)
            .await
        else {
            return;
        };
        let iface = iface_ref.get().await;
        let context = iface_ref.signal_context();
        let _ = iface.title_changed(context).await;
        let _ = iface.app_id_changed(context).await;
        let _ = iface.pid_changed(context).await;
        let _ = iface.workspace_changed(context).await;
        let _ = iface.output_changed(context).await;
        let _ = iface.focused_changed(context).await;
        let _ = iface.floating_changed(context).await;
        let _ = iface.urgent_changed(context).await;
        let _ = iface.column_index_changed(context).await;
        let _ = iface.row_index_changed(context).await;
        let _ = iface.tile_width_changed(context).await;
        let _ = iface.tile_height_changed(context).await;
        let _ = iface.tile_x_changed(context).await;
        let _ = iface.tile_y_changed(context).await;
        let _ = iface.window_width_changed(context).await;
        let _ = iface.window_height_changed(context).await;
        let _ = iface.window_offset_x_changed(context).await;
        let _ = iface.window_offset_y_changed(context).await;
        let _ = iface.focus_timestamp_secs_changed(context).await;
        let _ = iface.focus_timestamp_nanos_changed(context).await;
    }
}

fn sorted(values: HashSet<u64>) -> Vec<u64> {
    let mut values = values.into_iter().collect::<Vec<_>>();
    values.sort_unstable();
    values
}

fn sorted_strings(values: HashSet<String>) -> Vec<String> {
    let mut values = values.into_iter().collect::<Vec<_>>();
    values.sort();
    values
}
