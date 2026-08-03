use gtk::{self, gdk, prelude::*};
use nerd_icon_picker::{NerdIcon, NerdIconPicker};

fn main() {
    let app = gtk::Application::builder()
        .application_id("org.rsynapse.NerdIconPickerDemo")
        .build();
    app.connect_activate(build_demo);
    app.run();
}

fn build_demo(app: &gtk::Application) {
    install_demo_css();

    let picker = NerdIconPicker::new();
    picker.set_specific_icons(
        [
            ("Suggested Rust", "\u{e7a8}"),
            ("Suggested terminal", "\u{e795}"),
            ("Suggested folder", "\u{e5ff}"),
            ("Suggested app", "\u{f08c6}"),
            ("Suggested workspace", "\u{ebc3}"),
        ]
        .into_iter()
        .filter_map(|(name, glyph)| NerdIcon::specific(name, glyph))
        .collect(),
    );
    picker.set_selected_glyph(Some("\u{e7a8}"));
    picker.set_reset_visible(true);
    picker.set_query("rust");

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Nerd Icon Picker")
        .default_width(440)
        .child(picker.widget())
        .build();
    let picker_lifetime = picker.clone();
    window.connect_destroy(move |_| {
        let _ = &picker_lifetime;
    });
    window.present();
}

fn install_demo_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(
        ".nerd-icon-picker { padding: 16px; background: #1e1e2e; color: #cdd6f4; }\
         .nerd-icon-picker-section-label { font-weight: 700; }\
         .nerd-icon-picker-choice { padding: 9px; }\
         .nerd-icon-picker-choice.selected { background: #45475a; color: #cba6f7; }\
         .nerd-icon-picker-glyph { font-family: 'JetBrainsMono Nerd Font'; font-size: 22px; }\
         .nerd-icon-picker-status { margin: 6px; }",
    );
    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().expect("GTK display"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
