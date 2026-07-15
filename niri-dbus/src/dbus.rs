use std::sync::Arc;

use niri_ipc::{Mode, Output, Transform};
use tokio::sync::RwLock;
use zbus::interface;
use zbus::zvariant::OwnedObjectPath;

use niri_dbus::paths;

use crate::state::NiriState;

pub type SharedState = Arc<RwLock<NiriState>>;

#[derive(Clone)]
pub struct RootInterface {
    state: SharedState,
}

impl RootInterface {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }
}

#[interface(name = "org.rsynapse.Niri1")]
impl RootInterface {
    #[zbus(property)]
    async fn connected(&self) -> bool {
        self.state.read().await.connected
    }

    #[zbus(property)]
    async fn compositor_version(&self) -> String {
        self.state.read().await.compositor_version.clone()
    }

    #[zbus(property)]
    async fn generation(&self) -> u64 {
        self.state.read().await.generation
    }

    #[zbus(property)]
    async fn outputs(&self) -> Vec<OwnedObjectPath> {
        self.state.read().await.output_paths()
    }

    #[zbus(property)]
    async fn workspaces(&self) -> Vec<OwnedObjectPath> {
        self.state.read().await.workspace_paths()
    }

    #[zbus(property)]
    async fn windows(&self) -> Vec<OwnedObjectPath> {
        self.state.read().await.window_paths()
    }

    #[zbus(property)]
    async fn focused_output(&self) -> Vec<OwnedObjectPath> {
        optional_path(self.state.read().await.focused_output_path())
    }

    #[zbus(property)]
    async fn focused_workspace(&self) -> Vec<OwnedObjectPath> {
        optional_path(self.state.read().await.focused_workspace_path())
    }

    #[zbus(property)]
    async fn focused_window(&self) -> Vec<OwnedObjectPath> {
        optional_path(self.state.read().await.focused_window_path())
    }

    #[zbus(property)]
    async fn keyboard_layouts(&self) -> Vec<String> {
        self.state
            .read()
            .await
            .keyboard_layouts()
            .map(|layouts| layouts.names.clone())
            .unwrap_or_default()
    }

    #[zbus(property)]
    async fn keyboard_layout_index(&self) -> u8 {
        self.state
            .read()
            .await
            .keyboard_layouts()
            .map(|layouts| layouts.current_idx)
            .unwrap_or(0)
    }

    #[zbus(property)]
    async fn overview_open(&self) -> bool {
        self.state.read().await.overview_open()
    }

    #[zbus(property)]
    async fn config_load_failed(&self) -> bool {
        self.state.read().await.config_load_failed()
    }
}

#[derive(Clone)]
pub struct OutputInterface {
    state: SharedState,
    name: String,
}

impl OutputInterface {
    pub fn new(state: SharedState, name: String) -> Self {
        Self { state, name }
    }
}

#[interface(name = "org.rsynapse.Niri1.Output")]
impl OutputInterface {
    #[zbus(property)]
    async fn name(&self) -> String {
        self.name.clone()
    }

    #[zbus(property)]
    async fn make(&self) -> String {
        self.state
            .read()
            .await
            .output(&self.name)
            .map(|output| output.make.clone())
            .unwrap_or_default()
    }

    #[zbus(property)]
    async fn model(&self) -> String {
        self.state
            .read()
            .await
            .output(&self.name)
            .map(|output| output.model.clone())
            .unwrap_or_default()
    }

    #[zbus(property)]
    async fn serial(&self) -> Vec<String> {
        self.state
            .read()
            .await
            .output(&self.name)
            .and_then(|output| output.serial.clone())
            .into_iter()
            .collect()
    }

    #[zbus(property)]
    async fn focused(&self) -> bool {
        self.state.read().await.focused_output_name().as_deref() == Some(self.name.as_str())
    }

    #[zbus(property)]
    async fn current_workspace(&self) -> Vec<OwnedObjectPath> {
        optional_path(
            self.state
                .read()
                .await
                .current_workspace_for_output(&self.name),
        )
    }

    #[zbus(property)]
    async fn workspaces(&self) -> Vec<OwnedObjectPath> {
        self.state.read().await.workspaces_for_output(&self.name)
    }

    #[zbus(property)]
    async fn physical_width_mm(&self) -> Vec<u32> {
        self.state
            .read()
            .await
            .output(&self.name)
            .and_then(|output| output.physical_size.map(|size| size.0))
            .into_iter()
            .collect()
    }

    #[zbus(property)]
    async fn physical_height_mm(&self) -> Vec<u32> {
        self.state
            .read()
            .await
            .output(&self.name)
            .and_then(|output| output.physical_size.map(|size| size.1))
            .into_iter()
            .collect()
    }

    #[zbus(property)]
    async fn current_mode_width(&self) -> Vec<u16> {
        self.state
            .read()
            .await
            .output(&self.name)
            .and_then(current_mode)
            .map(|mode| mode.width)
            .into_iter()
            .collect()
    }

    #[zbus(property)]
    async fn current_mode_height(&self) -> Vec<u16> {
        self.state
            .read()
            .await
            .output(&self.name)
            .and_then(current_mode)
            .map(|mode| mode.height)
            .into_iter()
            .collect()
    }

    #[zbus(property)]
    async fn current_mode_refresh_mhz(&self) -> Vec<u32> {
        self.state
            .read()
            .await
            .output(&self.name)
            .and_then(current_mode)
            .map(|mode| mode.refresh_rate)
            .into_iter()
            .collect()
    }

    #[zbus(property)]
    async fn current_mode_preferred(&self) -> bool {
        self.state
            .read()
            .await
            .output(&self.name)
            .and_then(current_mode)
            .map(|mode| mode.is_preferred)
            .unwrap_or(false)
    }

    #[zbus(property)]
    async fn custom_mode(&self) -> bool {
        self.state
            .read()
            .await
            .output(&self.name)
            .map(|output| output.is_custom_mode)
            .unwrap_or(false)
    }

    #[zbus(property)]
    async fn logical_x(&self) -> i32 {
        self.state
            .read()
            .await
            .output(&self.name)
            .and_then(|output| output.logical)
            .map(|logical| logical.x)
            .unwrap_or(0)
    }

    #[zbus(property)]
    async fn logical_y(&self) -> i32 {
        self.state
            .read()
            .await
            .output(&self.name)
            .and_then(|output| output.logical)
            .map(|logical| logical.y)
            .unwrap_or(0)
    }

    #[zbus(property)]
    async fn logical_width(&self) -> u32 {
        self.state
            .read()
            .await
            .output(&self.name)
            .and_then(|output| output.logical)
            .map(|logical| logical.width)
            .unwrap_or(0)
    }

    #[zbus(property)]
    async fn logical_height(&self) -> u32 {
        self.state
            .read()
            .await
            .output(&self.name)
            .and_then(|output| output.logical)
            .map(|logical| logical.height)
            .unwrap_or(0)
    }

    #[zbus(property)]
    async fn scale(&self) -> f64 {
        self.state
            .read()
            .await
            .output(&self.name)
            .and_then(|output| output.logical)
            .map(|logical| logical.scale)
            .unwrap_or(1.0)
    }

    #[zbus(property)]
    async fn transform(&self) -> String {
        self.state
            .read()
            .await
            .output(&self.name)
            .and_then(|output| output.logical)
            .map(|logical| transform_name(logical.transform).to_owned())
            .unwrap_or_default()
    }

    #[zbus(property)]
    async fn vrr_supported(&self) -> bool {
        self.state
            .read()
            .await
            .output(&self.name)
            .map(|output| output.vrr_supported)
            .unwrap_or(false)
    }

    #[zbus(property)]
    async fn vrr_enabled(&self) -> bool {
        self.state
            .read()
            .await
            .output(&self.name)
            .map(|output| output.vrr_enabled)
            .unwrap_or(false)
    }
}

#[derive(Clone)]
pub struct WorkspaceInterface {
    state: SharedState,
    id: u64,
}

impl WorkspaceInterface {
    pub fn new(state: SharedState, id: u64) -> Self {
        Self { state, id }
    }
}

#[interface(name = "org.rsynapse.Niri1.Workspace")]
impl WorkspaceInterface {
    #[zbus(property)]
    async fn id(&self) -> u64 {
        self.id
    }

    #[zbus(property)]
    async fn name(&self) -> Vec<String> {
        self.state
            .read()
            .await
            .workspace(self.id)
            .and_then(|workspace| workspace.name.clone())
            .into_iter()
            .collect()
    }

    #[zbus(property)]
    async fn index(&self) -> u8 {
        self.state
            .read()
            .await
            .workspace(self.id)
            .map(|workspace| workspace.idx)
            .unwrap_or(0)
    }

    #[zbus(property)]
    async fn output(&self) -> Vec<OwnedObjectPath> {
        optional_path(
            self.state
                .read()
                .await
                .workspace(self.id)
                .and_then(|workspace| workspace.output.as_ref())
                .map(|output| paths::output_path(output)),
        )
    }

    #[zbus(property)]
    async fn active(&self) -> bool {
        self.state
            .read()
            .await
            .workspace(self.id)
            .map(|workspace| workspace.is_active)
            .unwrap_or(false)
    }

    #[zbus(property)]
    async fn focused(&self) -> bool {
        self.state
            .read()
            .await
            .workspace(self.id)
            .map(|workspace| workspace.is_focused)
            .unwrap_or(false)
    }

    #[zbus(property)]
    async fn urgent(&self) -> bool {
        self.state
            .read()
            .await
            .workspace(self.id)
            .map(|workspace| workspace.is_urgent)
            .unwrap_or(false)
    }

    #[zbus(property)]
    async fn active_window(&self) -> Vec<OwnedObjectPath> {
        optional_path(
            self.state
                .read()
                .await
                .workspace(self.id)
                .and_then(|workspace| workspace.active_window_id)
                .map(paths::window_path),
        )
    }

    #[zbus(property)]
    async fn windows(&self) -> Vec<OwnedObjectPath> {
        self.state.read().await.windows_for_workspace(self.id)
    }
}

#[derive(Clone)]
pub struct WindowInterface {
    state: SharedState,
    id: u64,
}

impl WindowInterface {
    pub fn new(state: SharedState, id: u64) -> Self {
        Self { state, id }
    }
}

#[interface(name = "org.rsynapse.Niri1.Window")]
impl WindowInterface {
    #[zbus(property)]
    async fn id(&self) -> u64 {
        self.id
    }

    #[zbus(property(emits_changed_signal = "false"))]
    async fn title(&self) -> Vec<String> {
        self.state
            .read()
            .await
            .window(self.id)
            .and_then(|window| window.title.clone())
            .into_iter()
            .collect()
    }

    #[zbus(property)]
    async fn app_id(&self) -> Vec<String> {
        self.state
            .read()
            .await
            .window(self.id)
            .and_then(|window| window.app_id.clone())
            .into_iter()
            .collect()
    }

    #[zbus(property)]
    async fn pid(&self) -> Vec<i32> {
        self.state
            .read()
            .await
            .window(self.id)
            .and_then(|window| window.pid)
            .into_iter()
            .collect()
    }

    #[zbus(property)]
    async fn workspace(&self) -> Vec<OwnedObjectPath> {
        optional_path(
            self.state
                .read()
                .await
                .window(self.id)
                .and_then(|window| window.workspace_id)
                .map(paths::workspace_path),
        )
    }

    #[zbus(property)]
    async fn output(&self) -> Vec<OwnedObjectPath> {
        let state = self.state.read().await;
        optional_path(
            state
                .window(self.id)
                .and_then(|window| state.output_for_window(window)),
        )
    }

    #[zbus(property)]
    async fn focused(&self) -> bool {
        self.state
            .read()
            .await
            .window(self.id)
            .map(|window| window.is_focused)
            .unwrap_or(false)
    }

    #[zbus(property)]
    async fn floating(&self) -> bool {
        self.state
            .read()
            .await
            .window(self.id)
            .map(|window| window.is_floating)
            .unwrap_or(false)
    }

    #[zbus(property)]
    async fn urgent(&self) -> bool {
        self.state
            .read()
            .await
            .window(self.id)
            .map(|window| window.is_urgent)
            .unwrap_or(false)
    }

    #[zbus(property)]
    async fn column_index(&self) -> Vec<u64> {
        self.state
            .read()
            .await
            .window(self.id)
            .and_then(|window| window.layout.pos_in_scrolling_layout)
            .map(|position| position.0 as u64)
            .into_iter()
            .collect()
    }

    #[zbus(property)]
    async fn row_index(&self) -> Vec<u64> {
        self.state
            .read()
            .await
            .window(self.id)
            .and_then(|window| window.layout.pos_in_scrolling_layout)
            .map(|position| position.1 as u64)
            .into_iter()
            .collect()
    }

    #[zbus(property)]
    async fn tile_width(&self) -> f64 {
        self.state
            .read()
            .await
            .window(self.id)
            .map(|window| window.layout.tile_size.0)
            .unwrap_or(0.0)
    }

    #[zbus(property)]
    async fn tile_height(&self) -> f64 {
        self.state
            .read()
            .await
            .window(self.id)
            .map(|window| window.layout.tile_size.1)
            .unwrap_or(0.0)
    }

    #[zbus(property)]
    async fn tile_x(&self) -> Vec<f64> {
        self.state
            .read()
            .await
            .window(self.id)
            .and_then(|window| window.layout.tile_pos_in_workspace_view)
            .map(|position| position.0)
            .into_iter()
            .collect()
    }

    #[zbus(property)]
    async fn tile_y(&self) -> Vec<f64> {
        self.state
            .read()
            .await
            .window(self.id)
            .and_then(|window| window.layout.tile_pos_in_workspace_view)
            .map(|position| position.1)
            .into_iter()
            .collect()
    }

    #[zbus(property)]
    async fn window_width(&self) -> i32 {
        self.state
            .read()
            .await
            .window(self.id)
            .map(|window| window.layout.window_size.0)
            .unwrap_or(0)
    }

    #[zbus(property)]
    async fn window_height(&self) -> i32 {
        self.state
            .read()
            .await
            .window(self.id)
            .map(|window| window.layout.window_size.1)
            .unwrap_or(0)
    }

    #[zbus(property)]
    async fn window_offset_x(&self) -> f64 {
        self.state
            .read()
            .await
            .window(self.id)
            .map(|window| window.layout.window_offset_in_tile.0)
            .unwrap_or(0.0)
    }

    #[zbus(property)]
    async fn window_offset_y(&self) -> f64 {
        self.state
            .read()
            .await
            .window(self.id)
            .map(|window| window.layout.window_offset_in_tile.1)
            .unwrap_or(0.0)
    }

    #[zbus(property(emits_changed_signal = "false"))]
    async fn focus_timestamp_secs(&self) -> Vec<u64> {
        self.state
            .read()
            .await
            .window(self.id)
            .and_then(|window| window.focus_timestamp)
            .map(|timestamp| timestamp.secs)
            .into_iter()
            .collect()
    }

    #[zbus(property(emits_changed_signal = "false"))]
    async fn focus_timestamp_nanos(&self) -> Vec<u32> {
        self.state
            .read()
            .await
            .window(self.id)
            .and_then(|window| window.focus_timestamp)
            .map(|timestamp| timestamp.nanos)
            .into_iter()
            .collect()
    }
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

fn optional_path(path: Option<OwnedObjectPath>) -> Vec<OwnedObjectPath> {
    path.into_iter().collect()
}
