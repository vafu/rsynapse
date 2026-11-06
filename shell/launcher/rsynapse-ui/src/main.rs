mod dbus;

use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    time::Duration,
};

use gtk4_layer_shell::{KeyboardMode, Layer, LayerShell};
use relm4::adw;
use relm4::adw::prelude::*;
use relm4::gtk;
use relm4::gtk::gdk;
use relm4::gtk::glib;
use relm4::prelude::*;
use rsynapse_launcher_api::{
    KeyEvent as LauncherKeyEvent, KeyRequest, KeyResponse, LauncherHint, LauncherMode,
    LauncherResult, LauncherView, SearchRequest, SearchResponse, ShowRequest,
};
use shell_core::keybind::{
    HoldAction, HoldBindings, HoldWatcher, KeyChord, Keybind, KeybindWatcher,
};

const MAX_ITEMS: usize = 10;
const RSYNAPSE_UI_CSS_PRIORITY: u32 = gtk::STYLE_PROVIDER_PRIORITY_USER + 1;

struct App {
    window: gtk::Window,
    revealer: gtk::Revealer,
    transition_generation: Rc<Cell<u64>>,
    title: gtk::Label,
    subtitle: gtk::Label,
    search_entry: gtk::SearchEntry,
    list_box: gtk::ListBox,
    scrolled: gtk::ScrolledWindow,
    hints: gtk::Label,
    params: Rc<RefCell<Vec<String>>>,
    results: Vec<LauncherResult>,
    selected: i32,
    mode: LauncherMode,
    preselect_next: bool,
    search_generation: Rc<Cell<u64>>,
    search_tx: std::sync::mpsc::Sender<(u64, Vec<String>, String)>,
    _hold_watcher: HoldWatcher,
    _nav_watcher: KeybindWatcher<Msg>,
}

#[derive(Clone, Debug)]
enum Msg {
    SearchResults {
        generation: u64,
        response: SearchResponse,
    },
    SearchError {
        generation: u64,
        message: String,
    },
    Cycle(i32),
    Activate,
    Hide,
    Show(ShowRequest),
    Toggle(ShowRequest),
    ForwardKey(LauncherKeyEvent),
    KeyResponse(KeyResponse),
    Refresh,
}

enum UiCommand {
    Show(ShowRequest),
    Toggle(ShowRequest),
    Hide,
}

#[relm4::component]
impl SimpleComponent for App {
    type Init = (Vec<String>, LauncherMode, bool);
    type Input = Msg;
    type Output = ();

    view! {
        #[root]
        gtk::Window {}
    }

    fn init(
        (params, mode, preselect_next): Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.init_layer_shell();
        root.set_namespace(Some("rsynapse-ui"));
        root.set_layer(Layer::Overlay);
        root.set_keyboard_mode(KeyboardMode::Exclusive);
        root.set_decorated(false);
        root.add_css_class("workspace-selector-window");

        let widgets = view_output!();

        let title = gtk::Label::new(Some("Rsynapse"));
        title.add_css_class("workspace-selector-title");
        title.set_xalign(0.0);

        let subtitle = gtk::Label::new(None);
        subtitle.add_css_class("workspace-selector-subtitle");
        subtitle.set_xalign(0.0);
        subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
        subtitle.set_max_width_chars(72);
        subtitle.set_visible(false);

        let header = gtk::Box::new(gtk::Orientation::Vertical, 2);
        header.add_css_class("workspace-selector-header");
        header.append(&title);
        header.append(&subtitle);

        let search_entry = gtk::SearchEntry::new();
        search_entry.set_property("search-delay", 80u32);
        search_entry.set_placeholder_text(Some("Search"));

        let list_box = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .activate_on_single_click(true)
            .css_classes(["boxed-list", "workspace-selector-list"])
            .build();

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .propagate_natural_height(true)
            .max_content_height(380)
            .build();
        scrolled.set_child(Some(&list_box));
        scrolled.set_visible(false);

        let hints = gtk::Label::new(None);
        hints.add_css_class("dim-label");
        hints.add_css_class("workspace-selector-hint");
        hints.set_xalign(0.0);
        hints.set_visible(false);

        let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
        card.add_css_class("workspace-selector");
        card.set_width_request(560);
        card.append(&header);
        card.append(&search_entry);
        card.append(&scrolled);
        card.append(&hints);

        let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
        frame.add_css_class("workspace-selector-frame");
        frame.set_halign(gtk::Align::Center);
        frame.set_valign(gtk::Align::Center);
        frame.append(&card);

        let revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::Crossfade)
            .transition_duration(100)
            .reveal_child(true)
            .build();
        revealer.set_child(Some(&frame));
        root.set_child(Some(&revealer));

        let search_generation = Rc::new(Cell::new(0u64));
        let (search_tx, search_rx) = std::sync::mpsc::channel::<(u64, Vec<String>, String)>();
        let worker_sender = sender.clone();
        std::thread::spawn(move || {
            while let Ok(mut job) = search_rx.recv() {
                while let Ok(newer) = search_rx.try_recv() {
                    job = newer;
                }
                let (generation, params, query) = job;
                let worker = worker_sender.clone();
                match dbus::search(SearchRequest {
                    params,
                    query,
                    selected: String::new(),
                }) {
                    Ok(response) => {
                        glib::idle_add_once(move || {
                            worker.input(Msg::SearchResults {
                                generation,
                                response,
                            });
                        });
                    }
                    Err(error) => {
                        let message = error.to_string();
                        glib::idle_add_once(move || {
                            worker.input(Msg::SearchError {
                                generation,
                                message,
                            });
                        });
                    }
                }
            }
        });

        let params = Rc::new(RefCell::new(params));
        let search_params = params.clone();
        let search_tx_entry = search_tx.clone();
        let search_generation_entry = search_generation.clone();
        search_entry.connect_search_changed(move |entry| {
            let generation = search_generation_entry.get().wrapping_add(1);
            search_generation_entry.set(generation);
            let _ = search_tx_entry.send((
                generation,
                search_params.borrow().clone(),
                entry.text().to_string(),
            ));
        });

        let activate_sender = sender.clone();
        search_entry.connect_activate(move |_| activate_sender.input(Msg::Activate));
        let activate_sender = sender.clone();
        list_box.connect_row_activated(move |_, _| activate_sender.input(Msg::Activate));

        let nav_sender = sender.clone();
        let nav_watcher = KeybindWatcher::install(
            &root,
            [
                Keybind::new(KeyChord::any_modifiers(gdk::Key::Up), Msg::Cycle(-1)),
                Keybind::new(KeyChord::any_modifiers(gdk::Key::Down), Msg::Cycle(1)),
            ],
            move |message| nav_sender.input(message),
        );

        let hold_sender = sender.clone();
        let hold_bindings = HoldBindings {
            next: Vec::new(),
            previous: Vec::new(),
            ..HoldBindings::default()
        };
        let hold_watcher = HoldWatcher::install(&root, hold_bindings, move |action| {
            let message = match action {
                HoldAction::Next => Msg::Cycle(1),
                HoldAction::Previous => Msg::Cycle(-1),
                HoldAction::Activate | HoldAction::ModifierReleased => Msg::Activate,
                HoldAction::Cancel => Msg::Hide,
            };
            hold_sender.input(message);
        });
        if mode == LauncherMode::Cycle {
            hold_watcher.start();
        }

        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let s = sender.clone();
        let key_params = params.clone();
        let key_search_entry = search_entry.clone();
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            if key == gdk::Key::Escape {
                s.input(Msg::Hide);
                return glib::Propagation::Stop;
            }
            if !key_search_entry.is_visible() && !key_params.borrow().is_empty() {
                if key == gdk::Key::j {
                    s.input(Msg::Cycle(1));
                    return glib::Propagation::Stop;
                }
                if key == gdk::Key::k {
                    s.input(Msg::Cycle(-1));
                    return glib::Propagation::Stop;
                }
            }
            let forward =
                matches!(key, gdk::Key::Delete) || modifiers.contains(gdk::ModifierType::ALT_MASK);
            if !forward {
                return glib::Propagation::Proceed;
            }
            s.input(Msg::ForwardKey(LauncherKeyEvent {
                key: key_name(key),
                ctrl: modifiers.contains(gdk::ModifierType::CONTROL_MASK),
                alt: modifiers.contains(gdk::ModifierType::ALT_MASK),
                shift: modifiers.contains(gdk::ModifierType::SHIFT_MASK),
            }));
            glib::Propagation::Stop
        });
        root.add_controller(keys);

        // D-Bus toggle interface
        let (dbus_tx, dbus_rx) = async_channel::unbounded::<UiCommand>();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                let conn = match zbus::Connection::session().await {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("[rsynapse-ui] D-Bus connect failed: {}", e);
                        return;
                    }
                };
                if let Err(e) = conn
                    .object_server()
                    .at("/org/rsynapse/UI", UiDbus { sender: dbus_tx })
                    .await
                {
                    eprintln!("[rsynapse-ui] D-Bus object_server failed: {}", e);
                    return;
                }
                if let Err(e) = conn.request_name("org.rsynapse.UIToggle").await {
                    eprintln!("[rsynapse-ui] D-Bus request_name failed: {}", e);
                    return;
                }
                eprintln!("[rsynapse-ui] D-Bus toggle interface registered");
                loop {
                    std::future::pending::<()>().await;
                }
            });
        });

        let toggle_sender = sender.clone();
        glib::spawn_future_local(async move {
            while let Ok(command) = dbus_rx.recv().await {
                let message = match command {
                    UiCommand::Show(request) => Msg::Show(request),
                    UiCommand::Toggle(request) => Msg::Toggle(request),
                    UiCommand::Hide => Msg::Hide,
                };
                let _ = toggle_sender.input(message);
            }
        });

        root.present();
        list_box.set_focusable(true);
        if mode == LauncherMode::Cycle {
            search_entry.set_visible(false);
            let list_box = list_box.clone();
            glib::idle_add_local_once(move || {
                list_box.grab_focus();
            });
        } else {
            search_entry.grab_focus();
        }

        let model = App {
            window: root.clone(),
            revealer,
            transition_generation: Rc::new(Cell::new(0)),
            title,
            subtitle,
            search_entry,
            list_box,
            scrolled,
            hints,
            params: params.clone(),
            results: Vec::new(),
            selected: 0,
            mode,
            preselect_next,
            search_generation: search_generation.clone(),
            search_tx: search_tx.clone(),
            _hold_watcher: hold_watcher,
            _nav_watcher: nav_watcher,
        };
        model.start_search(&sender);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Msg::SearchError {
                generation,
                message,
            } => {
                if generation != self.search_generation.get() {
                    return;
                }
                self.results.clear();
                self.selected = 0;
                self.refresh_list();
                self.subtitle.set_text(&format!("Search failed: {message}"));
                self.subtitle.set_visible(true);
            }
            Msg::SearchResults {
                generation,
                response,
            } => {
                if generation != self.search_generation.get() {
                    return;
                }
                self.apply_view(&response.view);
                let mut results = response.results;
                results.truncate(MAX_ITEMS);
                self.results = results;
                self.selected = if self.preselect_next && self.results.len() > 1 {
                    1
                } else {
                    0
                };
                self.refresh_list();
            }
            Msg::Cycle(delta) => self.cycle(delta),
            Msg::Activate => self.activate_selected(&sender),
            Msg::Refresh => self.start_search(&sender),
            Msg::Hide => self.hide(),
            Msg::Show(request) => self.show_request(request, &sender),
            Msg::Toggle(request) => {
                if self.window.is_visible() {
                    if request.mode == LauncherMode::Cycle {
                        let same_request = self.mode == request.mode
                            && self.preselect_next == request.preselect_next
                            && *self.params.borrow() == request.params;
                        if same_request {
                            self._hold_watcher.start();
                            self.inhibit_shortcuts();
                            self.cycle(1);
                        } else {
                            self.show_request(request, &sender);
                        }
                    } else {
                        self.hide();
                    }
                } else {
                    self.show_request(request, &sender);
                }
            }
            Msg::ForwardKey(event) => {
                let request = KeyRequest {
                    params: self.params.borrow().clone(),
                    query: self.search_entry.text().to_string(),
                    selected: self
                        .results
                        .get(self.selected as usize)
                        .map(|result| result.id.clone())
                        .unwrap_or_default(),
                    event,
                };
                let s = sender.clone();
                std::thread::spawn(move || {
                    let result = dbus::handle_key(request).unwrap_or_default();
                    glib::idle_add_once(move || {
                        s.input(Msg::KeyResponse(result));
                    });
                });
            }
            Msg::KeyResponse(response) => {
                if !response.handled {
                    return;
                }
                if !response.message.is_empty() {
                    self.subtitle.set_text(&response.message);
                    self.subtitle.set_visible(true);
                }
                if response.refresh {
                    self.start_search(&sender);
                }
                if response.close {
                    self.hide();
                }
            }
        }
    }
}

impl App {
    fn start_search(&self, _sender: &ComponentSender<Self>) {
        let generation = self.search_generation.get().wrapping_add(1);
        self.search_generation.set(generation);
        let _ = self.search_tx.send((
            generation,
            self.params.borrow().clone(),
            self.search_entry.text().to_string(),
        ));
    }

    fn show_request(&mut self, request: ShowRequest, sender: &ComponentSender<Self>) {
        *self.params.borrow_mut() = request.params;
        self.mode = request.mode;
        self.preselect_next = request.preselect_next;
        let cycle = self.mode == LauncherMode::Cycle;
        let query = request.query;
        let changed = self.search_entry.text() != query;
        self.search_entry.set_text(&query);
        self.transition_generation
            .set(self.transition_generation.get().wrapping_add(1));
        self.window.set_keyboard_mode(KeyboardMode::Exclusive);
        self.window.set_visible(true);
        self.revealer.set_reveal_child(true);
        self.search_entry.set_visible(!cycle);
        if cycle {
            self._hold_watcher.start();
            self.inhibit_shortcuts();
            self.list_box.grab_focus();
        } else {
            self._hold_watcher.stop();
            self.search_entry.grab_focus();
        }
        if !changed {
            self.start_search(sender);
        }
    }

    fn hide(&mut self) {
        self.mode = LauncherMode::Search;
        self._hold_watcher.stop();
        self.transition_generation
            .set(self.transition_generation.get().wrapping_add(1));
        let generation = self.transition_generation.get();
        self.window.set_keyboard_mode(KeyboardMode::None);
        self.search_entry.set_text("");
        self.revealer.set_reveal_child(false);
        let window = self.window.downgrade();
        let current = self.transition_generation.clone();
        glib::timeout_add_local_once(Duration::from_millis(110), move || {
            if current.get() == generation {
                if let Some(window) = window.upgrade() {
                    window.set_visible(false);
                }
            }
        });
    }

    fn inhibit_shortcuts(&self) {
        if let Some(surface) = self.window.surface() {
            if let Ok(toplevel) = surface.downcast::<gdk::Toplevel>() {
                toplevel.inhibit_system_shortcuts(None::<&gdk::Event>);
            }
        }
    }

    fn cycle(&mut self, delta: i32) {
        if self.results.is_empty() {
            return;
        }
        let count = self.results.len() as i32;
        self.selected = (self.selected + delta).rem_euclid(count);
        self.update_selection();
    }

    fn activate_selected(&mut self, sender: &ComponentSender<Self>) {
        let Some(result) = self.results.get(self.selected as usize).cloned() else {
            return;
        };
        let keep_open = result.keep_open;
        if !keep_open {
            // Release the layer-shell keyboard grab before activating another
            // output's workspace or window. The compositor must be free to move
            // focus away from this launcher.
            self.hide();
        }
        eprintln!("[rsynapse-ui] Executing: {} ({})", result.title, result.id);
        let sender = sender.clone();
        std::thread::spawn(move || {
            match dbus::execute(&result.id) {
                Ok(r) => eprintln!("[rsynapse-ui] Execute result: {:?}", r),
                Err(e) => eprintln!("[rsynapse-ui] Execute error: {}", e),
            }
            if keep_open {
                let _ = sender.input(Msg::Refresh);
            }
        });
    }

    fn apply_view(&self, view: &LauncherView) {
        self.title.set_text(if view.title.is_empty() {
            "Rsynapse"
        } else {
            &view.title
        });
        self.subtitle.set_text(&view.subtitle);
        self.subtitle.set_visible(!view.subtitle.is_empty());
        self.search_entry
            .set_placeholder_text(Some(if view.placeholder.is_empty() {
                "Search"
            } else {
                &view.placeholder
            }));
        self.apply_hints(&view.hints);
    }

    fn apply_hints(&self, hints: &[LauncherHint]) {
        let text = hints
            .iter()
            .map(|hint| format!("{} {}", hint.key, hint.label))
            .collect::<Vec<_>>()
            .join(" · ");
        self.hints.set_text(&text);
        self.hints.set_visible(!hints.is_empty());
        self.window.queue_resize();
    }

    fn refresh_list(&mut self) {
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }

        for result in &self.results {
            let row = adw::ActionRow::builder()
                .title(&result.title)
                .subtitle(&result.description)
                .title_lines(1)
                .subtitle_lines(1)
                .activatable(true)
                .build();
            row.add_css_class("workspace-selector-row");

            if !result.icon.is_empty() {
                row.add_prefix(&result_icon(&result.icon));
            }
            if result.current {
                let check = gtk::Image::from_icon_name("object-select-symbolic");
                check.add_css_class("accent");
                check.set_valign(gtk::Align::Center);
                row.add_suffix(&check);
            }

            self.list_box.append(&row);
        }

        self.update_selection();
        self.scrolled.set_visible(!self.results.is_empty());
        self.window.queue_resize();
    }

    fn update_selection(&self) {
        if let Some(row) = self.list_box.row_at_index(self.selected) {
            self.list_box.select_row(Some(&row));
            shell_core::list::scroll_row_into_view(&self.scrolled, &row);
        } else {
            self.list_box.unselect_all();
        }
    }
}

fn result_icon(icon: &str) -> gtk::Widget {
    if let Some(path) = icon_file_path(icon) {
        if path.is_file() {
            let image = gtk::Image::from_file(&path);
            image.set_pixel_size(16);
            image.set_halign(gtk::Align::Center);
            image.set_valign(gtk::Align::Center);
            return image.upcast();
        }
        return gtk::Image::from_icon_name("application-x-executable-symbolic").upcast();
    }
    if is_theme_icon(icon) {
        gtk::Image::from_icon_name(icon).upcast()
    } else {
        let label = gtk::Label::new(Some(icon));
        label.add_css_class("launcher-icon");
        label.set_valign(gtk::Align::Center);
        label.upcast()
    }
}

fn icon_file_path(icon: &str) -> Option<PathBuf> {
    let path = icon.strip_prefix("file://").unwrap_or(icon);
    let path = path.strip_prefix("localhost").unwrap_or(path);
    path.starts_with('/').then(|| PathBuf::from(path))
}

fn is_theme_icon(icon: &str) -> bool {
    !icon.is_empty()
        && icon
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+'))
}

fn key_name(key: gdk::Key) -> String {
    match key {
        gdk::Key::Delete => "Delete".to_owned(),
        gdk::Key::BackSpace => "Backspace".to_owned(),
        gdk::Key::Tab => "Tab".to_owned(),
        gdk::Key::Return | gdk::Key::KP_Enter => "Enter".to_owned(),
        gdk::Key::Escape => "Escape".to_owned(),
        other => other
            .name()
            .map(|name| name.to_string())
            .unwrap_or_default(),
    }
}

struct UiDbus {
    sender: async_channel::Sender<UiCommand>,
}

#[zbus::interface(name = "org.rsynapse.UIToggle1")]
impl UiDbus {
    fn show(&self, request: ShowRequest) {
        let _ = self.sender.try_send(UiCommand::Show(request));
    }

    fn toggle(&self, request: ShowRequest) {
        let _ = self.sender.try_send(UiCommand::Toggle(request));
    }

    fn hide(&self) {
        let _ = self.sender.try_send(UiCommand::Hide);
    }
}

fn try_toggle_existing(params: &[String], mode: LauncherMode, preselect_next: bool) -> bool {
    let Ok(connection) = zbus::blocking::Connection::session() else {
        return false;
    };
    let Ok(proxy) = rsynapse_launcher_api::UiToggleProxyBlocking::new(&connection) else {
        return false;
    };
    proxy
        .toggle(ShowRequest {
            params: params.to_vec(),
            query: String::new(),
            mode,
            preselect_next,
        })
        .is_ok()
}

fn install_css() {
    let Some(display) = gdk::Display::default() else {
        eprintln!("[rsynapse-ui] No GDK display available for CSS");
        return;
    };

    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("style.css"));
    gtk::style_context_add_provider_for_display(&display, &provider, RSYNAPSE_UI_CSS_PRIORITY);
}

fn main() {
    let mut params = Vec::new();
    let mut mode = LauncherMode::Search;
    let mut preselect_next = false;
    for arg in std::env::args().skip(1) {
        if matches!(arg.as_str(), "--hold" | "--cycle") {
            mode = LauncherMode::Cycle;
        } else if arg == "--preselect-next" {
            preselect_next = true;
        } else if matches!(arg.as_str(), "--current-output" | "--current-monitor") {
            params.push("current-output".to_owned());
        } else {
            params.push(arg);
        }
    }
    if try_toggle_existing(&params, mode, preselect_next) {
        return;
    }

    adw::init().expect("Failed to init libadwaita");

    let binary = std::env::args().next().unwrap_or_default();
    let app = RelmApp::new("org.rsynapse.UI").with_args(vec![binary]);
    install_css();
    app.run::<App>((params, mode, preselect_next));
}
