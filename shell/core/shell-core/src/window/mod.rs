mod config;
mod layer;

pub use layer::{apply_layer_shell_config, create_layer_window};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Edge {
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Layer {
    Background,
    Bottom,
    Top,
    Overlay,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub struct SurfaceMargins {
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub left: i32,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Anchors {
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
    pub left: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ExclusiveZone {
    None,
    Fixed(i32),
    Auto,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct WindowConfig {
    pub layer: Layer,
    pub anchors: Anchors,
    /// Layer-shell surface offsets from screen edges.
    ///
    /// These affect compositor placement and exclusive-zone behavior. Consumers
    /// should use CSS margins/padding for spacing inside the GTK window.
    pub surface_margins: SurfaceMargins,
    pub exclusive_zone: ExclusiveZone,
    pub namespace: Option<&'static str>,
    pub keyboard_interactive: bool,
}

#[cfg(test)]
mod test;
