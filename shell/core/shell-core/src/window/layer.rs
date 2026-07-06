use gtk4_layer_shell::{Edge as GtkEdge, KeyboardMode, Layer as GtkLayer, LayerShell};

use super::{Anchors, Edge, ExclusiveZone, Layer, SurfaceMargins, WindowConfig};

pub fn create_layer_window(config: WindowConfig) -> gtk::Window {
    let window = gtk::Window::new();
    apply_layer_shell_config(&window, config);
    window
}

pub fn apply_layer_shell_config(
    window: &impl gtk::prelude::IsA<gtk::Window>,
    config: WindowConfig,
) {
    window.init_layer_shell();
    window.set_layer(config.layer.into());
    window.set_keyboard_mode(if config.keyboard_interactive {
        KeyboardMode::OnDemand
    } else {
        KeyboardMode::None
    });

    if let Some(namespace) = config.namespace {
        window.set_namespace(Some(namespace));
    }

    apply_anchors(window, config.anchors);
    apply_surface_margins(window, config.surface_margins);

    match config.exclusive_zone {
        ExclusiveZone::None => window.set_exclusive_zone(0),
        ExclusiveZone::Fixed(exclusive_zone) => window.set_exclusive_zone(exclusive_zone),
        ExclusiveZone::Auto => window.auto_exclusive_zone_enable(),
    }

    gtk4_background_effect::apply_background_effect(window, config.background_effect);
}

fn apply_anchors(window: &impl gtk::prelude::IsA<gtk::Window>, anchors: Anchors) {
    window.set_anchor(GtkEdge::Top, anchors.top);
    window.set_anchor(GtkEdge::Right, anchors.right);
    window.set_anchor(GtkEdge::Bottom, anchors.bottom);
    window.set_anchor(GtkEdge::Left, anchors.left);
}

fn apply_surface_margins(window: &impl gtk::prelude::IsA<gtk::Window>, margins: SurfaceMargins) {
    window.set_margin(GtkEdge::Top, margins.top);
    window.set_margin(GtkEdge::Right, margins.right);
    window.set_margin(GtkEdge::Bottom, margins.bottom);
    window.set_margin(GtkEdge::Left, margins.left);
}

impl From<Layer> for GtkLayer {
    fn from(layer: Layer) -> Self {
        match layer {
            Layer::Background => Self::Background,
            Layer::Bottom => Self::Bottom,
            Layer::Top => Self::Top,
            Layer::Overlay => Self::Overlay,
        }
    }
}

impl From<Edge> for GtkEdge {
    fn from(edge: Edge) -> Self {
        match edge {
            Edge::Top => Self::Top,
            Edge::Right => Self::Right,
            Edge::Bottom => Self::Bottom,
            Edge::Left => Self::Left,
        }
    }
}
