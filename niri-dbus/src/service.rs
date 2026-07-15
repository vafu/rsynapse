use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use niri_ipc::{Mode, Output, Transform};
use tokio::{sync::RwLock, time::sleep};
use tracing::{debug, info, warn};
use zbus::{Connection, connection::Builder, zvariant::OwnedObjectPath};

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

macro_rules! emit_changed {
    ($before:expr, $after:expr, $iface:expr, $context:expr, $field:ident, $signal:ident) => {
        if $before.$field != $after.$field {
            let _ = $iface.$signal($context).await;
        }
    };
}

impl Service {
    async fn run_niri_loop(mut self) -> anyhow::Result<()> {
        loop {
            match self.run_connected_once().await {
                Ok(()) => warn!("niri event stream ended"),
                Err(error) => warn!("niri connection failed: {error:#}"),
            }

            let before = self.snapshot().await;
            let delta = self.state.write().await.mark_disconnected();
            let after = self.snapshot().await;
            self.apply_object_delta(delta).await?;
            self.emit_changes(&before, &after).await;
            sleep(Duration::from_secs(1)).await;
        }
    }

    async fn run_connected_once(&mut self) -> anyhow::Result<()> {
        let before = self.snapshot().await;
        let (version, outputs) = ipc::initial_snapshot().await?;
        let delta = self.state.write().await.mark_connected(version, outputs);
        let after = self.snapshot().await;
        self.apply_object_delta(delta).await?;
        self.emit_changes(&before, &after).await;

        let mut stream = ipc::event_stream().await?;
        loop {
            let event = stream.read_event().await?;
            debug!(?event, "niri event");
            let before = self.snapshot().await;
            let delta = self.state.write().await.apply_event(event)?;
            let after = self.snapshot().await;
            self.apply_object_delta(delta).await?;
            self.emit_changes(&before, &after).await;
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

    async fn snapshot(&self) -> ProjectionSnapshot {
        let state = self.state.read().await;
        ProjectionSnapshot::from_state(&state)
    }

    async fn emit_changes(&self, before: &ProjectionSnapshot, after: &ProjectionSnapshot) {
        self.emit_root_changes(&before.root, &after.root).await;
        for (output, after) in after.outputs.iter() {
            if let Some(before) = before.outputs.get(output) {
                self.emit_output_changes(output, before, after).await;
            }
        }
        for (workspace, after) in after.workspaces.iter() {
            if let Some(before) = before.workspaces.get(workspace) {
                self.emit_workspace_changes(*workspace, before, after).await;
            }
        }
        for (window, after) in after.windows.iter() {
            if let Some(before) = before.windows.get(window) {
                self.emit_window_changes(*window, before, after).await;
            }
        }
    }

    async fn emit_root_changes(&self, before: &RootProjection, after: &RootProjection) {
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
        emit_changed!(before, after, iface, context, connected, connected_changed);
        emit_changed!(
            before,
            after,
            iface,
            context,
            compositor_version,
            compositor_version_changed
        );
        emit_changed!(
            before,
            after,
            iface,
            context,
            generation,
            generation_changed
        );
        emit_changed!(before, after, iface, context, outputs, outputs_changed);
        emit_changed!(
            before,
            after,
            iface,
            context,
            workspaces,
            workspaces_changed
        );
        emit_changed!(before, after, iface, context, windows, windows_changed);
        emit_changed!(
            before,
            after,
            iface,
            context,
            focused_output,
            focused_output_changed
        );
        emit_changed!(
            before,
            after,
            iface,
            context,
            focused_workspace,
            focused_workspace_changed
        );
        emit_changed!(
            before,
            after,
            iface,
            context,
            focused_window,
            focused_window_changed
        );
        emit_changed!(
            before,
            after,
            iface,
            context,
            keyboard_layouts,
            keyboard_layouts_changed
        );
        emit_changed!(
            before,
            after,
            iface,
            context,
            keyboard_layout_index,
            keyboard_layout_index_changed
        );
        emit_changed!(
            before,
            after,
            iface,
            context,
            overview_open,
            overview_open_changed
        );
        emit_changed!(
            before,
            after,
            iface,
            context,
            config_load_failed,
            config_load_failed_changed
        );
    }

    async fn emit_output_changes(
        &self,
        output: &str,
        before: &OutputProjection,
        after: &OutputProjection,
    ) {
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
        emit_changed!(before, after, iface, context, make, make_changed);
        emit_changed!(before, after, iface, context, model, model_changed);
        emit_changed!(before, after, iface, context, serial, serial_changed);
        emit_changed!(before, after, iface, context, focused, focused_changed);
        emit_changed!(
            before,
            after,
            iface,
            context,
            current_workspace,
            current_workspace_changed
        );
        emit_changed!(
            before,
            after,
            iface,
            context,
            workspaces,
            workspaces_changed
        );
        emit_changed!(
            before,
            after,
            iface,
            context,
            physical_width_mm,
            physical_width_mm_changed
        );
        emit_changed!(
            before,
            after,
            iface,
            context,
            physical_height_mm,
            physical_height_mm_changed
        );
        emit_changed!(
            before,
            after,
            iface,
            context,
            current_mode_width,
            current_mode_width_changed
        );
        emit_changed!(
            before,
            after,
            iface,
            context,
            current_mode_height,
            current_mode_height_changed
        );
        emit_changed!(
            before,
            after,
            iface,
            context,
            current_mode_refresh_mhz,
            current_mode_refresh_mhz_changed
        );
        emit_changed!(
            before,
            after,
            iface,
            context,
            current_mode_preferred,
            current_mode_preferred_changed
        );
        emit_changed!(
            before,
            after,
            iface,
            context,
            custom_mode,
            custom_mode_changed
        );
        emit_changed!(before, after, iface, context, logical_x, logical_x_changed);
        emit_changed!(before, after, iface, context, logical_y, logical_y_changed);
        emit_changed!(
            before,
            after,
            iface,
            context,
            logical_width,
            logical_width_changed
        );
        emit_changed!(
            before,
            after,
            iface,
            context,
            logical_height,
            logical_height_changed
        );
        emit_changed!(before, after, iface, context, scale, scale_changed);
        emit_changed!(before, after, iface, context, transform, transform_changed);
        emit_changed!(
            before,
            after,
            iface,
            context,
            vrr_supported,
            vrr_supported_changed
        );
        emit_changed!(
            before,
            after,
            iface,
            context,
            vrr_enabled,
            vrr_enabled_changed
        );
    }

    async fn emit_workspace_changes(
        &self,
        workspace: u64,
        before: &WorkspaceProjection,
        after: &WorkspaceProjection,
    ) {
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
        emit_changed!(before, after, iface, context, name, name_changed);
        emit_changed!(before, after, iface, context, index, index_changed);
        emit_changed!(before, after, iface, context, output, output_changed);
        emit_changed!(before, after, iface, context, active, active_changed);
        emit_changed!(before, after, iface, context, focused, focused_changed);
        emit_changed!(before, after, iface, context, urgent, urgent_changed);
        emit_changed!(
            before,
            after,
            iface,
            context,
            active_window,
            active_window_changed
        );
        emit_changed!(before, after, iface, context, windows, windows_changed);
    }

    async fn emit_window_changes(
        &self,
        window: u64,
        before: &WindowProjection,
        after: &WindowProjection,
    ) {
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
        emit_changed!(before, after, iface, context, app_id, app_id_changed);
        emit_changed!(before, after, iface, context, pid, pid_changed);
        emit_changed!(before, after, iface, context, workspace, workspace_changed);
        emit_changed!(before, after, iface, context, output, output_changed);
        emit_changed!(before, after, iface, context, focused, focused_changed);
        emit_changed!(before, after, iface, context, floating, floating_changed);
        emit_changed!(before, after, iface, context, urgent, urgent_changed);
        emit_changed!(
            before,
            after,
            iface,
            context,
            column_index,
            column_index_changed
        );
        emit_changed!(before, after, iface, context, row_index, row_index_changed);
        emit_changed!(
            before,
            after,
            iface,
            context,
            tile_width,
            tile_width_changed
        );
        emit_changed!(
            before,
            after,
            iface,
            context,
            tile_height,
            tile_height_changed
        );
        emit_changed!(before, after, iface, context, tile_x, tile_x_changed);
        emit_changed!(before, after, iface, context, tile_y, tile_y_changed);
        emit_changed!(
            before,
            after,
            iface,
            context,
            window_width,
            window_width_changed
        );
        emit_changed!(
            before,
            after,
            iface,
            context,
            window_height,
            window_height_changed
        );
        emit_changed!(
            before,
            after,
            iface,
            context,
            window_offset_x,
            window_offset_x_changed
        );
        emit_changed!(
            before,
            after,
            iface,
            context,
            window_offset_y,
            window_offset_y_changed
        );
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ProjectionSnapshot {
    root: RootProjection,
    outputs: HashMap<String, OutputProjection>,
    workspaces: HashMap<u64, WorkspaceProjection>,
    windows: HashMap<u64, WindowProjection>,
}

impl ProjectionSnapshot {
    fn from_state(state: &NiriState) -> Self {
        Self {
            root: RootProjection::from_state(state),
            outputs: state
                .outputs
                .keys()
                .map(|name| (name.clone(), OutputProjection::from_state(state, name)))
                .collect(),
            workspaces: state
                .event_state
                .workspaces
                .workspaces
                .keys()
                .map(|id| (*id, WorkspaceProjection::from_state(state, *id)))
                .collect(),
            windows: state
                .event_state
                .windows
                .windows
                .keys()
                .map(|id| (*id, WindowProjection::from_state(state, *id)))
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct RootProjection {
    connected: bool,
    compositor_version: String,
    generation: u64,
    outputs: Vec<OwnedObjectPath>,
    workspaces: Vec<OwnedObjectPath>,
    windows: Vec<OwnedObjectPath>,
    focused_output: Option<OwnedObjectPath>,
    focused_workspace: Option<OwnedObjectPath>,
    focused_window: Option<OwnedObjectPath>,
    keyboard_layouts: Vec<String>,
    keyboard_layout_index: u8,
    overview_open: bool,
    config_load_failed: bool,
}

impl RootProjection {
    fn from_state(state: &NiriState) -> Self {
        let keyboard_layouts = state.keyboard_layouts();
        Self {
            connected: state.connected,
            compositor_version: state.compositor_version.clone(),
            generation: state.generation,
            outputs: state.output_paths(),
            workspaces: state.workspace_paths(),
            windows: state.window_paths(),
            focused_output: state.focused_output_path(),
            focused_workspace: state.focused_workspace_path(),
            focused_window: state.focused_window_path(),
            keyboard_layouts: keyboard_layouts
                .map(|layouts| layouts.names.clone())
                .unwrap_or_default(),
            keyboard_layout_index: keyboard_layouts
                .map(|layouts| layouts.current_idx)
                .unwrap_or(0),
            overview_open: state.overview_open(),
            config_load_failed: state.config_load_failed(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct OutputProjection {
    make: String,
    model: String,
    serial: Option<String>,
    focused: bool,
    current_workspace: Option<OwnedObjectPath>,
    workspaces: Vec<OwnedObjectPath>,
    physical_width_mm: Option<u32>,
    physical_height_mm: Option<u32>,
    current_mode_width: Option<u16>,
    current_mode_height: Option<u16>,
    current_mode_refresh_mhz: Option<u32>,
    current_mode_preferred: bool,
    custom_mode: bool,
    logical_x: i32,
    logical_y: i32,
    logical_width: u32,
    logical_height: u32,
    scale: f64,
    transform: String,
    vrr_supported: bool,
    vrr_enabled: bool,
}

impl OutputProjection {
    fn from_state(state: &NiriState, name: &str) -> Self {
        let output = state.output(name);
        let current_mode = output.and_then(current_mode);
        let logical = output.and_then(|output| output.logical);
        Self {
            make: output.map(|output| output.make.clone()).unwrap_or_default(),
            model: output
                .map(|output| output.model.clone())
                .unwrap_or_default(),
            serial: output.and_then(|output| output.serial.clone()),
            focused: state.focused_output_name().as_deref() == Some(name),
            current_workspace: state.current_workspace_for_output(name),
            workspaces: state.workspaces_for_output(name),
            physical_width_mm: output.and_then(|output| output.physical_size.map(|size| size.0)),
            physical_height_mm: output.and_then(|output| output.physical_size.map(|size| size.1)),
            current_mode_width: current_mode.map(|mode| mode.width),
            current_mode_height: current_mode.map(|mode| mode.height),
            current_mode_refresh_mhz: current_mode.map(|mode| mode.refresh_rate),
            current_mode_preferred: current_mode.map(|mode| mode.is_preferred).unwrap_or(false),
            custom_mode: output.map(|output| output.is_custom_mode).unwrap_or(false),
            logical_x: logical.map(|logical| logical.x).unwrap_or(0),
            logical_y: logical.map(|logical| logical.y).unwrap_or(0),
            logical_width: logical.map(|logical| logical.width).unwrap_or(0),
            logical_height: logical.map(|logical| logical.height).unwrap_or(0),
            scale: logical.map(|logical| logical.scale).unwrap_or(1.0),
            transform: logical
                .map(|logical| transform_name(logical.transform).to_owned())
                .unwrap_or_default(),
            vrr_supported: output.map(|output| output.vrr_supported).unwrap_or(false),
            vrr_enabled: output.map(|output| output.vrr_enabled).unwrap_or(false),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct WorkspaceProjection {
    name: Option<String>,
    index: u8,
    output: Option<OwnedObjectPath>,
    active: bool,
    focused: bool,
    urgent: bool,
    active_window: Option<OwnedObjectPath>,
    windows: Vec<OwnedObjectPath>,
}

impl WorkspaceProjection {
    fn from_state(state: &NiriState, id: u64) -> Self {
        let workspace = state.workspace(id);
        Self {
            name: workspace.and_then(|workspace| workspace.name.clone()),
            index: workspace.map(|workspace| workspace.idx).unwrap_or(0),
            output: workspace
                .and_then(|workspace| workspace.output.as_ref())
                .map(|output| paths::output_path(output)),
            active: workspace
                .map(|workspace| workspace.is_active)
                .unwrap_or(false),
            focused: workspace
                .map(|workspace| workspace.is_focused)
                .unwrap_or(false),
            urgent: workspace
                .map(|workspace| workspace.is_urgent)
                .unwrap_or(false),
            active_window: workspace
                .and_then(|workspace| workspace.active_window_id)
                .map(paths::window_path),
            windows: state.windows_for_workspace(id),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct WindowProjection {
    app_id: Option<String>,
    pid: Option<i32>,
    workspace: Option<OwnedObjectPath>,
    output: Option<OwnedObjectPath>,
    focused: bool,
    floating: bool,
    urgent: bool,
    column_index: Option<u64>,
    row_index: Option<u64>,
    tile_width: f64,
    tile_height: f64,
    tile_x: Option<f64>,
    tile_y: Option<f64>,
    window_width: i32,
    window_height: i32,
    window_offset_x: f64,
    window_offset_y: f64,
}

impl WindowProjection {
    fn from_state(state: &NiriState, id: u64) -> Self {
        let window = state.window(id);
        let layout = window.map(|window| &window.layout);
        Self {
            app_id: window.and_then(|window| window.app_id.clone()),
            pid: window.and_then(|window| window.pid),
            workspace: window
                .and_then(|window| window.workspace_id)
                .map(paths::workspace_path),
            output: window.and_then(|window| state.output_for_window(window)),
            focused: window.map(|window| window.is_focused).unwrap_or(false),
            floating: window.map(|window| window.is_floating).unwrap_or(false),
            urgent: window.map(|window| window.is_urgent).unwrap_or(false),
            column_index: layout
                .and_then(|layout| layout.pos_in_scrolling_layout)
                .map(|position| position.0 as u64),
            row_index: layout
                .and_then(|layout| layout.pos_in_scrolling_layout)
                .map(|position| position.1 as u64),
            tile_width: layout.map(|layout| layout.tile_size.0).unwrap_or(0.0),
            tile_height: layout.map(|layout| layout.tile_size.1).unwrap_or(0.0),
            tile_x: layout
                .and_then(|layout| layout.tile_pos_in_workspace_view)
                .map(|position| position.0),
            tile_y: layout
                .and_then(|layout| layout.tile_pos_in_workspace_view)
                .map(|position| position.1),
            window_width: layout.map(|layout| layout.window_size.0).unwrap_or(0),
            window_height: layout.map(|layout| layout.window_size.1).unwrap_or(0),
            window_offset_x: layout
                .map(|layout| layout.window_offset_in_tile.0)
                .unwrap_or(0.0),
            window_offset_y: layout
                .map(|layout| layout.window_offset_in_tile.1)
                .unwrap_or(0.0),
        }
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

fn current_mode(output: &Output) -> Option<Mode> {
    output
        .current_mode
        .and_then(|index| output.modes.get(index).copied())
}

fn transform_name(transform: Transform) -> &'static str {
    match transform {
        Transform::Normal => "normal",
        Transform::_90 => "90",
        Transform::_180 => "180",
        Transform::_270 => "270",
        Transform::Flipped => "flipped",
        Transform::Flipped90 => "flipped-90",
        Transform::Flipped180 => "flipped-180",
        Transform::Flipped270 => "flipped-270",
    }
}
