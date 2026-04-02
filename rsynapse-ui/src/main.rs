mod dbus;

use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::adw;
use relm4::adw::prelude::*;
use relm4::gtk;
use relm4::gtk::gdk;
use relm4::gtk::glib;
use relm4::gtk::prelude::*;
use relm4::prelude::*;

const MAX_ITEMS: usize = 10;

struct App {
    window: gtk::Window,
    search_entry: gtk::SearchEntry,
    list_box: gtk::ListBox,
    scrolled: gtk::ScrolledWindow,
    results: Vec<dbus::SearchResult>,
    selected: i32,
}

#[derive(Debug)]
enum Msg {
    SearchResults(Vec<dbus::SearchResult>),
    SelectNext,
    SelectPrev,
    Activate,
    Hide,
    Toggle,
}

#[relm4::component]
impl SimpleComponent for App {
    type Init = ();
    type Input = Msg;
    type Output = ();

    view! {
        #[root]
        gtk::Window {}
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.init_layer_shell();
        root.set_layer(Layer::Overlay);
        root.set_keyboard_mode(KeyboardMode::Exclusive);
        root.set_anchor(Edge::Top, true);
        root.set_anchor(Edge::Bottom, true);
        root.set_anchor(Edge::Left, false);
        root.set_anchor(Edge::Right, false);
        root.set_decorated(false);
        root.add_css_class("rsynapse-window");

        let widgets = view_output!();

        let search_entry = gtk::SearchEntry::builder()
            .css_classes(["rsynapse-search"])
            .build();
        search_entry.set_property("search-delay", 0u32);

        let list_box = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["navigation-sidebar"])
            .build();

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .css_classes(["rsynapse-items"])
            .build();
        scrolled.set_child(Some(&list_box));
        scrolled.set_visible(false);

        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .valign(gtk::Align::End)
            .css_classes(["rsynapse"])
            .build();
        container.append(&scrolled);
        container.append(&search_entry);

        root.set_child(Some(&container));

        // Search changed — run D-Bus call in background thread
        let s = sender.clone();
        search_entry.connect_search_changed(move |entry| {
            let query = entry.text().to_string();
            let s2 = s.clone();
            std::thread::spawn(move || {
                let results = if query.trim().is_empty() {
                    Vec::new()
                } else {
                    dbus::search(&query).unwrap_or_default()
                };
                glib::idle_add_once(move || {
                    s2.input(Msg::SearchResults(results));
                });
            });
        });

        // Key controller on the search entry
        let s = sender.clone();
        let key_ctrl = gtk::EventControllerKey::new();
        key_ctrl.connect_key_pressed(move |_, keyval, _, _| {
            match keyval {
                gdk::Key::Down => { s.input(Msg::SelectNext); glib::Propagation::Stop }
                gdk::Key::Up => { s.input(Msg::SelectPrev); glib::Propagation::Stop }
                gdk::Key::Return => { s.input(Msg::Activate); glib::Propagation::Stop }
                gdk::Key::Escape => { s.input(Msg::Hide); glib::Propagation::Stop }
                _ => glib::Propagation::Proceed
            }
        });
        search_entry.add_controller(key_ctrl);

        // D-Bus toggle interface
        let (dbus_tx, dbus_rx) = std::sync::mpsc::channel::<()>();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                let conn = match zbus::Connection::session().await {
                    Ok(c) => c,
                    Err(e) => { eprintln!("[rsynapse-ui] D-Bus connect failed: {}", e); return; }
                };
                if let Err(e) = conn.object_server()
                    .at("/org/rsynapse/UI", UiDbus { sender: dbus_tx })
                    .await {
                    eprintln!("[rsynapse-ui] D-Bus object_server failed: {}", e);
                    return;
                }
                if let Err(e) = conn.request_name("org.rsynapse.UIToggle").await {
                    eprintln!("[rsynapse-ui] D-Bus request_name failed: {}", e);
                    return;
                }
                eprintln!("[rsynapse-ui] D-Bus toggle interface registered");
                loop { std::future::pending::<()>().await; }
            });
        });

        let toggle_sender = sender.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            if dbus_rx.try_recv().is_ok() {
                toggle_sender.input(Msg::Toggle);
            }
            glib::ControlFlow::Continue
        });

        root.present();
        search_entry.grab_focus();

        let model = App {
            window: root.clone(),
            search_entry,
            list_box,
            scrolled,
            results: Vec::new(),
            selected: 0,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            Msg::SearchResults(mut results) => {
                results.truncate(MAX_ITEMS);
                self.results = results;
                self.selected = 0;
                self.refresh_list();
            }
            Msg::SelectNext => {
                if !self.results.is_empty() {
                    self.selected = (self.selected + 1).min(self.results.len() as i32 - 1);
                    self.update_selection();
                }
            }
            Msg::SelectPrev => {
                if !self.results.is_empty() {
                    self.selected = (self.selected - 1).max(0);
                    self.update_selection();
                }
            }
            Msg::Activate => {
                if let Some(result) = self.results.get(self.selected as usize) {
                    dbus::execute(&result.id).ok();
                    self.search_entry.set_text("");
                    self.window.set_visible(false);
                }
            }
            Msg::Hide => {
                self.search_entry.set_text("");
                self.window.set_visible(false);
            }
            Msg::Toggle => {
                if self.window.is_visible() {
                    self.search_entry.set_text("");
                    self.window.set_visible(false);
                } else {
                    self.window.set_visible(true);
                    self.search_entry.grab_focus();
                }
            }
        }
    }
}

impl App {
    fn refresh_list(&mut self) {
        // Clear
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }

        for result in &self.results {
            let row = adw::ActionRow::builder()
                .title(&result.title)
                .subtitle(&result.description)
                .build();

            if !result.icon.is_empty() {
                row.add_prefix(&gtk::Image::from_icon_name(&result.icon));
            }

            self.list_box.append(&row);
        }

        self.scrolled.set_visible(!self.results.is_empty());
        self.update_selection();
    }

    fn update_selection(&self) {
        if let Some(row) = self.list_box.row_at_index(self.selected) {
            self.list_box.select_row(Some(&row));
        }
    }
}

struct UiDbus {
    sender: std::sync::mpsc::Sender<()>,
}

#[zbus::interface(name = "org.rsynapse.UIToggle1")]
impl UiDbus {
    fn toggle(&self) {
        self.sender.send(()).ok();
    }
}

fn try_toggle_existing() -> bool {
    std::process::Command::new("busctl")
        .args(["--user", "call", "org.rsynapse.UIToggle", "/org/rsynapse/UI", "org.rsynapse.UIToggle1", "Toggle"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn main() {
    if try_toggle_existing() {
        return;
    }

    adw::init().expect("Failed to init libadwaita");

    let app = RelmApp::new("org.rsynapse.UI");
    relm4::set_global_css(include_str!("style.css"));
    app.run::<App>(());
}
