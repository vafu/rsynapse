//! Rsynapse's shell-like task-and-notes overlay (v2).
use adw::prelude::*;
use gtk::{gdk, glib, prelude::WidgetExt};
use relm4::prelude::*;
use shell_core::{
    gtk4_layer_shell::{KeyboardMode, LayerShell},
    window::{Anchors, Layer, WindowConfig},
};

/// Input messages for the placeholder remarked overlay.
#[derive(Debug, Clone)]
pub enum RemarkedInput {
    Show,
    Hide,
    Toggle,
}

/// A transparent fullscreen layer-shell overlay for tasks and notes.
pub struct RemarkedWindow {
    window: adw::ApplicationWindow,
    visible: bool,
}

impl RemarkedWindow {
    fn new(window: adw::ApplicationWindow) -> Self {
        Self {
            window,
            visible: false,
        }
    }

    fn show(&mut self) {
        self.fit_to_monitor();
        self.visible = true;
        self.window.present();
        self.window.set_keyboard_mode(KeyboardMode::Exclusive);
    }

    fn hide(&mut self) {
        self.visible = false;
        self.window.set_keyboard_mode(KeyboardMode::None);
        self.window.set_visible(false);
    }

    fn fit_to_monitor(&self) {
        let display = WidgetExt::display(&self.window);
        let monitors = display.monitors();
        let mut bounds = gdk::Rectangle::new(0, 0, i32::MAX, i32::MAX);
        for i in 0..monitors.n_items() {
            if let Some(monitor) = monitors.item(i).and_downcast::<gdk::Monitor>() {
                let geo = monitor.geometry();
                bounds = gdk::Rectangle::new(
                    bounds.x().max(geo.x()),
                    bounds.y().max(geo.y()),
                    bounds.width().min(geo.width()),
                    bounds.height().min(geo.height()),
                );
            }
        }
        let (width, height) = (
            bounds.width().clamp(1, i32::MAX),
            bounds.height().clamp(1, i32::MAX),
        );
        self.window.set_default_size(width, height);
    }
}

#[relm4::component(pub)]
impl SimpleComponent for RemarkedWindow {
    type Init = ();
    type Input = RemarkedInput;
    type Output = ();

    view! {
        #[root]
        adw::ApplicationWindow {
            add_css_class: "remarked-window",
            set_title: Some("Remarked"),
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        shell_core::window::apply_layer_shell_config(
            &root,
            WindowConfig::new(Layer::Overlay)
                .with_anchors(Anchors::ALL)
                .with_namespace("rsynapse-remarked"),
        );
        root.set_decorated(false);
        load_styles(&root);

        let fixed = gtk::Fixed::new();
        fixed.add_css_class("remarked-root");
        root.set_content(Some(&fixed));

        add_escape_handler(&root, sender);

        let model = Self::new(root.clone());
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            RemarkedInput::Show => self.show(),
            RemarkedInput::Hide => self.hide(),
            RemarkedInput::Toggle => {
                if self.visible {
                    self.hide();
                } else {
                    self.show();
                }
            }
        }
    }
}

fn load_styles(window: &adw::ApplicationWindow) {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        "
        .remarked-window {
            background: transparent;
        }
        .remarked-root {
            background: transparent;
        }
        .remarked-card {
            background-color: rgba(30, 30, 30, 0.75);
            border-radius: 18px;
            padding: 16px;
            border: 1px solid rgba(255, 255, 255, 0.08);
        }
        .remarked-card label {
            color: rgba(255, 255, 255, 0.9);
        }
        .remarked-card label.heading {
            font-weight: bold;
            font-size: 1.1em;
            margin-bottom: 8px;
        }
        ",
    );
    gtk::style_context_add_provider_for_display(
        &WidgetExt::display(window),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn add_escape_handler(window: &adw::ApplicationWindow, sender: ComponentSender<RemarkedWindow>) {
    let controller = gtk::EventControllerKey::new();
    controller.connect_key_pressed(move |_, keyval, _keycode, _mods| {
        if keyval == gtk::gdk::Key::Escape {
            sender.input(RemarkedInput::Hide);
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(controller);
}
