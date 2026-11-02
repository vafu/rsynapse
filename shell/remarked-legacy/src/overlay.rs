use adw::prelude::*;
use shell_core::{
    gtk4_layer_shell::{KeyboardMode, LayerShell},
    window::{self, Anchors, Layer, WindowConfig},
};

pub(super) fn configure(window: &adw::ApplicationWindow) {
    window::apply_layer_shell_config(
        window,
        WindowConfig::new(Layer::Overlay)
            .with_anchors(Anchors::NONE)
            .with_namespace("rsynapse-workspace"),
    );
    window.set_decorated(false);
}

pub(super) fn show(window: &adw::ApplicationWindow) {
    if !window.is_visible() {
        // An unspecified output follows the compositor's current output when the
        // surface is mapped. Fit every available logical monitor without moving
        // or resizing any normal workspace windows.
        window.set_monitor(None::<&gtk::gdk::Monitor>);
        let monitors = gtk::prelude::WidgetExt::display(window).monitors();
        let bounds = (0..monitors.n_items())
            .filter_map(|i| monitors.item(i).and_downcast::<gtk::gdk::Monitor>())
            .map(|monitor| monitor.geometry())
            .fold((i32::MAX, i32::MAX), |(w, h), rect| {
                (w.min(rect.width()), h.min(rect.height()))
            });
        let (width, height) = size(bounds.0, bounds.1);
        window.set_default_size(width, height);
    }
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    window.present();
}

pub(super) fn hide(window: &adw::ApplicationWindow) {
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_visible(false);
}

fn size(width: i32, height: i32) -> (i32, i32) {
    (
        (width.saturating_sub(96)).clamp(1, 1180),
        (height.saturating_sub(96)).clamp(1, 780),
    )
}
