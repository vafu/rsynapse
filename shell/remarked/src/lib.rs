//! Rsynapse's shell-like task-and-notes overlay (v2).
use adw::prelude::*;
use gtk::{gdk, glib, prelude::WidgetExt};
use relm4::prelude::*;
use shell_core::{
    gtk4_layer_shell::{KeyboardMode, LayerShell},
    source::StateSignal,
    window::{Anchors, Layer, WindowConfig},
};

mod client;
mod source;

/// Input messages for the remarked overlay.
#[derive(Debug)]
pub enum RemarkedInput {
    Source(sources::Msg),
    Show,
    Hide,
    Toggle,
    ScrollToPage(Page),
    SelectTask(String),
    ClearTask,
    SelectNote(String),
    ClearNote,
    CaptureNote(String),
}

impl From<sources::Msg> for RemarkedInput {
    fn from(value: sources::Msg) -> Self {
        Self::Source(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Tasks,
    Notes,
}

/// A transparent fullscreen layer-shell overlay for tasks and notes.
#[shell_macros::model]
pub struct RemarkedWindow {
    window: adw::ApplicationWindow,
    carousel: adw::Carousel,
    task_page: gtk::Box,
    note_page: gtk::Box,
    task_revealer: gtk::Revealer,
    task_box: gtk::Box,
    task_list: gtk::ListBox,
    note_list: gtk::ListBox,
    note_filter_button: gtk::Button,
    note_status_label: gtk::Label,
    project_chip: gtk::Box,
    project_label: gtk::Label,
    task_chip_label: gtk::Label,
    note_sidebar_button: gtk::Button,
    note_sidebar_revealer: gtk::Revealer,
    project_id: Option<String>,
    notebook: StateSignal<Option<source::Notebook>>,
    selected_task: StateSignal<Option<String>>,
    selected_note: StateSignal<Option<String>>,
    visible: bool,
    #[source(source::catalog(shell_core::source::once(project_id.clone())))]
    catalog: source::Catalog,
    #[source(source::notes(notebook.observable(), selected_task.observable()))]
    notes: source::Notes,
}

impl RemarkedWindow {
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

    fn scroll_to(&self, page: Page) {
        let child = match page {
            Page::Tasks => &self.task_page,
            Page::Notes => &self.note_page,
        };
        self.carousel.scroll_to(child, true);
    }

    fn select_task(&mut self, task: Option<String>) {
        self.selected_task.set(task.clone());
        self.task_box.set_visible(task.is_some());
        self.task_revealer.set_reveal_child(false);
    }

    fn set_note(&mut self, note: Option<String>) {
        self.selected_note.set(note.clone());
        let has_note = note.is_some();
        self.note_sidebar_button.set_sensitive(has_note);
        if !has_note {
            self.note_sidebar_revealer.set_reveal_child(false);
        }
    }

    fn fit_to_monitor(&self) {
        let display = WidgetExt::display(&self.window);
        let monitors = display.monitors();
        let (mut width, mut height) = (i32::MAX, i32::MAX);
        for i in 0..monitors.n_items() {
            if let Some(monitor) = monitors.item(i).and_downcast::<gdk::Monitor>() {
                let geo = monitor.geometry();
                width = width.min(geo.width());
                height = height.min(geo.height());
            }
        }
        self.window
            .set_default_size(width.clamp(1, i32::MAX), height.clamp(1, i32::MAX));
    }

    fn reconcile(&mut self, sender: &ComponentSender<Self>) {
        if let Some(project) = self.catalog.project() {
            self.project_label.set_label(&project.name);
            let cwd = &project.cwd;
            if !cwd.is_empty() {
                self.notebook.set(Some(source::Notebook {
                    path: format!("{cwd}/.notes"),
                    generation: 0,
                }));
            }
            self.rebuild_task_list(self.catalog.tasks(), sender);
        } else {
            self.project_label.set_label("Project");
            self.notebook.set(None);
            self.task_list.remove_all();
        }

        if let Some(selected) = self.selected_task.get() {
            let title = self
                .catalog
                .tasks()
                .iter()
                .find(|t| t.instance_id == selected)
                .map(|t| t.goal.title.as_str())
                .unwrap_or("Selected task");
            self.task_chip_label.set_label(title);
        }

        match &self.notes {
            source::Notes::Ready(_path, notes) => {
                self.note_status_label
                    .set_label(&format!("{} notes", notes.len()));
                self.rebuild_note_list(notes, sender);
            }
            source::Notes::Loading => {
                self.note_status_label.set_label("Loading notes...");
                self.note_list.remove_all();
            }
            source::Notes::NoNotebook => {
                self.note_status_label
                    .set_label("No notebook for this project");
                self.note_list.remove_all();
            }
            source::Notes::Error(error) => {
                self.note_status_label
                    .set_label(&format!("Notes error: {error}"));
                self.note_list.remove_all();
            }
        }

        let has_filter = self.selected_task.get().is_some();
        self.note_filter_button.set_visible(has_filter);

        self.clear_changed();
    }

    fn rebuild_task_list(
        &self,
        tasks: &[proj_model::GoalIdentityInfo],
        sender: &ComponentSender<Self>,
    ) {
        self.task_list.remove_all();
        for task in tasks {
            let row = adw::ActionRow::builder()
                .title(&task.goal.title)
                .subtitle(&format!("{} · {}", task.goal.status, task.key))
                .activatable(true)
                .build();
            let instance_id = task.instance_id.clone();
            let tx = sender.input_sender().clone();
            row.connect_activated(move |_| {
                let _ = tx.send(RemarkedInput::SelectTask(instance_id.clone()));
            });
            self.task_list.append(&row);
        }
    }

    fn rebuild_note_list(
        &self,
        notes: &[notebook_api::note::Note],
        sender: &ComponentSender<Self>,
    ) {
        self.note_list.remove_all();
        for note in notes {
            let row = adw::ActionRow::builder()
                .title(&note.title)
                .subtitle(&note.tags.join(", "))
                .activatable(true)
                .build();
            let id = note.id.to_string();
            let tx = sender.input_sender().clone();
            row.connect_activated(move |_| {
                let _ = tx.send(RemarkedInput::SelectNote(id.clone()));
            });
            self.note_list.append(&row);
        }
    }
}

#[shell_macros::component(model = RemarkedWindow)]
#[relm4::component(pub)]
impl SimpleComponent for RemarkedWindow {
    type Init = Option<String>;
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
        project_id: Self::Init,
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

        let carousel = adw::Carousel::builder()
            .allow_scroll_wheel(false)
            .allow_mouse_drag(false)
            .build();

        let (task_page, task_list) = build_task_pane(&sender);
        task_page.set_size_request(640, 400);
        let (note_page, note_list, note_filter_button, note_status_label) =
            build_note_pane(&sender);
        note_page.set_size_request(640, 400);

        carousel.append(&task_page);
        carousel.append(&note_page);

        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&carousel));

        let (project_chip, project_label) = chip("Project");
        project_chip.set_halign(gtk::Align::Start);
        project_chip.set_valign(gtk::Align::Start);
        project_chip.set_margin_top(24);
        project_chip.set_margin_start(24);
        project_chip.set_width_request(120);
        overlay.add_overlay(&project_chip);

        let (task_chip, task_revealer, task_chip_label) = build_task_chip(&sender);
        let task_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Start)
            .margin_top(24)
            .width_request(200)
            .build();
        task_box.append(&task_chip);
        task_box.append(&task_revealer);
        task_box.set_visible(false);
        overlay.add_overlay(&task_box);

        root.set_content(Some(&overlay));

        add_keyboard_handlers(&root, &sender);

        let note_sidebar_button = find_note_sidebar_button(&note_page);
        let note_sidebar_revealer = find_note_sidebar_revealer(&note_page);

        let model = Self::new(
            root.clone(),
            carousel,
            task_page,
            note_page,
            task_revealer,
            task_box,
            task_list,
            note_list,
            note_filter_button,
            note_status_label,
            project_chip,
            project_label,
            task_chip_label,
            note_sidebar_button,
            note_sidebar_revealer,
            project_id,
            StateSignal::new(None),
            StateSignal::new(None),
            StateSignal::new(None),
            false,
        );
        let widgets = view_output!();
        model.reconcile(&sender);
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            RemarkedInput::Source(message) => {
                Self::update(self, message);
                self.reconcile(&sender);
            }
            RemarkedInput::Show => self.show(),
            RemarkedInput::Hide => self.hide(),
            RemarkedInput::Toggle => {
                if self.visible {
                    self.hide();
                } else {
                    self.show();
                }
            }
            RemarkedInput::ScrollToPage(page) => self.scroll_to(page),
            RemarkedInput::SelectTask(id) => self.select_task(Some(id)),
            RemarkedInput::ClearTask => self.select_task(None),
            RemarkedInput::SelectNote(id) => self.set_note(Some(id)),
            RemarkedInput::ClearNote => self.set_note(None),
            RemarkedInput::CaptureNote(text) => {
                let text = text.trim();
                if text.is_empty() {
                    return;
                }
                if let source::Notes::Ready(path, _) = &self.notes {
                    let path = path.clone();
                    let text = text.to_owned();
                    relm4::spawn(async move {
                        match crate::client::NotesClient::open(&path, Vec::new()).await {
                            Ok(client) => {
                                if let Err(error) = client.capture(&text).await {
                                    eprintln!("[remarked] capture failed: {error}");
                                }
                            }
                            Err(error) => eprintln!("[remarked] open notebook failed: {error}"),
                        }
                    });
                }
            }
        }
    }
}

fn build_task_pane(_sender: &ComponentSender<RemarkedWindow>) -> (gtk::Box, gtk::ListBox) {
    let split = adw::OverlaySplitView::new();
    split.set_sidebar_position(gtk::PackType::End);
    split.set_show_sidebar(false);
    split.set_pin_sidebar(true);

    let list_header = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .css_classes(["toolbar"])
        .build();
    let title = gtk::Label::builder()
        .label("Tasks")
        .halign(gtk::Align::Start)
        .hexpand(true)
        .css_classes(["heading"])
        .build();
    let details_button = gtk::Button::builder()
        .icon_name("document-properties-symbolic")
        .tooltip_text("Show task details")
        .css_classes(["flat", "circular"])
        .build();
    list_header.append(&title);
    list_header.append(&details_button);

    let list_box = gtk::ListBox::builder()
        .css_classes(["boxed-list"])
        .selection_mode(gtk::SelectionMode::Single)
        .vexpand(true)
        .build();
    list_box.add_css_class("remarked-list");

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .width_request(360)
        .build();
    content.append(&list_header);
    content.append(&list_box);

    let details = adw::PreferencesGroup::builder()
        .title("Task details")
        .build();
    details.add(
        &adw::ActionRow::builder()
            .title("Status")
            .subtitle("planned")
            .build(),
    );
    details.add(
        &adw::ActionRow::builder()
            .title("Priority")
            .subtitle("medium")
            .build(),
    );
    details.add(
        &adw::ActionRow::builder()
            .title("Description")
            .subtitle("...")
            .build(),
    );

    split.set_content(Some(&content));
    split.set_sidebar(Some(&details));

    let split_for_button = split.clone();
    details_button.connect_clicked(move |_| {
        split_for_button.set_show_sidebar(!split_for_button.shows_sidebar());
    });

    let page = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    page.append(&split);
    (page, list_box)
}

fn build_note_pane(
    sender: &ComponentSender<RemarkedWindow>,
) -> (gtk::Box, gtk::ListBox, gtk::Button, gtk::Label) {
    let split = adw::OverlaySplitView::new();
    split.set_sidebar_position(gtk::PackType::Start);
    split.set_show_sidebar(true);
    split.set_pin_sidebar(true);

    let list_header = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .css_classes(["toolbar"])
        .build();
    let title = gtk::Label::builder()
        .label("Notes")
        .halign(gtk::Align::Start)
        .hexpand(true)
        .css_classes(["heading"])
        .build();

    let entry = gtk::Entry::builder()
        .placeholder_text("New note...")
        .hexpand(true)
        .build();
    let add_button = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("Capture note")
        .css_classes(["flat", "circular"])
        .build();
    let clear_button = gtk::Button::builder()
        .label("Clear filter")
        .css_classes(["flat"])
        .visible(false)
        .build();
    list_header.append(&title);
    list_header.append(&entry);
    list_header.append(&add_button);
    list_header.append(&clear_button);

    let status_label = gtk::Label::builder()
        .halign(gtk::Align::Start)
        .css_classes(["dim-label", "caption"])
        .build();

    let list_box = gtk::ListBox::builder()
        .css_classes(["boxed-list"])
        .selection_mode(gtk::SelectionMode::Single)
        .vexpand(true)
        .build();
    list_box.add_css_class("remarked-list");

    let sidebar = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .width_request(280)
        .build();
    sidebar.append(&list_header);
    sidebar.append(&status_label);
    sidebar.append(&list_box);

    let editor_overlay = gtk::Overlay::new();

    let buffer = sourceview5::Buffer::new(None);
    let view = sourceview5::View::with_buffer(&buffer);
    view.set_vexpand(true);
    view.set_hexpand(true);
    view.add_css_class("frame");

    let scrolled = gtk::ScrolledWindow::builder()
        .child(&view)
        .vexpand(true)
        .hexpand(true)
        .build();
    let clamp = adw::Clamp::builder()
        .child(&scrolled)
        .maximum_size(960)
        .build();
    editor_overlay.set_child(Some(&clamp));

    let status = adw::StatusPage::builder()
        .title("Select a note")
        .description("Choose a note from the sidebar or create a new one.")
        .icon_name("document-edit-symbolic")
        .vexpand(true)
        .build();
    editor_overlay.add_overlay(&status);

    let sidebar_revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideLeft)
        .build();
    let note_details = adw::PreferencesGroup::builder()
        .title("Note details")
        .build();
    note_details.add(
        &adw::ActionRow::builder()
            .title("Backlinks")
            .subtitle("...")
            .build(),
    );
    note_details.add(
        &adw::ActionRow::builder()
            .title("Tags")
            .subtitle("...")
            .build(),
    );
    note_details.add(
        &adw::ActionRow::builder()
            .title("Created")
            .subtitle("...")
            .build(),
    );
    sidebar_revealer.set_child(Some(&note_details));
    editor_overlay.add_overlay(&sidebar_revealer);

    let toggle = gtk::Button::builder()
        .icon_name("sidebar-show-symbolic")
        .tooltip_text("Toggle note details")
        .halign(gtk::Align::End)
        .valign(gtk::Align::End)
        .margin_end(24)
        .margin_bottom(24)
        .css_classes(["circular", "osd"])
        .sensitive(false)
        .build();
    editor_overlay.add_overlay(&toggle);

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(0)
        .build();
    content.append(&editor_overlay);

    split.set_sidebar(Some(&sidebar));
    split.set_content(Some(&content));

    let sidebar_revealer_for_toggle = sidebar_revealer.clone();
    toggle.connect_clicked(move |_| {
        sidebar_revealer_for_toggle.set_reveal_child(!sidebar_revealer_for_toggle.reveals_child());
    });

    let tx = sender.input_sender().clone();
    clear_button.connect_clicked(move |_| {
        let _ = tx.send(RemarkedInput::ClearTask);
    });

    let tx = sender.input_sender().clone();
    let entry_for_button = entry.clone();
    add_button.connect_clicked(move |_| {
        let text = entry_for_button.text().to_string();
        entry_for_button.set_text("");
        let _ = tx.send(RemarkedInput::CaptureNote(text));
    });

    let tx = sender.input_sender().clone();
    entry.connect_activate(move |entry| {
        let text = entry.text().to_string();
        entry.set_text("");
        let _ = tx.send(RemarkedInput::CaptureNote(text));
    });

    // Store references so the model can find them later.
    toggle.set_widget_name("note-sidebar-toggle");
    sidebar_revealer.set_widget_name("note-sidebar-revealer");

    let page = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    page.append(&split);
    (page, list_box, clear_button, status_label)
}

fn chip(label: &str) -> (gtk::Box, gtk::Label) {
    let chip = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .css_classes(["remarked-chip"])
        .build();
    let label = gtk::Label::builder().label(label).build();
    chip.append(&label);
    (chip, label)
}

fn build_task_chip(
    _sender: &ComponentSender<RemarkedWindow>,
) -> (gtk::Box, gtk::Revealer, gtk::Label) {
    let button = gtk::Button::builder()
        .label("Selected task")
        .css_classes(["remarked-chip"])
        .build();

    let revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .build();
    let details = adw::PreferencesGroup::builder()
        .title("Task metadata")
        .build();
    details.add(
        &adw::ActionRow::builder()
            .title("Status")
            .subtitle("planned")
            .build(),
    );
    details.add(
        &adw::ActionRow::builder()
            .title("Priority")
            .subtitle("medium")
            .build(),
    );
    details.add(
        &adw::ActionRow::builder()
            .title("Description")
            .subtitle("...")
            .build(),
    );
    revealer.set_child(Some(&details));

    let r = revealer.clone();
    button.connect_clicked(move |_| {
        r.set_reveal_child(!r.reveals_child());
    });

    let chip_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(["remarked-chip"])
        .build();
    chip_box.append(&button);

    let label = button
        .first_child()
        .and_downcast::<gtk::Label>()
        .expect("button label");

    (chip_box, revealer, label)
}

fn load_styles(window: &adw::ApplicationWindow) {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(
        "
        .remarked-window {
            background: transparent;
        }
        .remarked-root {
            background: transparent;
        }
        .remarked-chip {
            background-color: rgba(40, 40, 40, 0.85);
            border-radius: 9999px;
            padding: 8px 16px;
            border: 1px solid rgba(255, 255, 255, 0.08);
        }
        .remarked-chip label,
        .remarked-chip button {
            color: rgba(255, 255, 255, 0.9);
        }
        .remarked-pane {
            background-color: rgba(30, 30, 30, 0.75);
            border-radius: 24px;
            margin: 48px;
        }
        .remarked-pane > * {
            border-radius: 24px;
        }
        .remarked-list {
            background: transparent;
        }
        .remarked-list row {
            background: rgba(255, 255, 255, 0.05);
            border-radius: 12px;
            margin: 4px 8px;
        }
        ",
    );
    gtk::style_context_add_provider_for_display(
        &WidgetExt::display(window),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn add_keyboard_handlers(
    window: &adw::ApplicationWindow,
    sender: &ComponentSender<RemarkedWindow>,
) {
    let controller = gtk::EventControllerKey::new();
    let s = sender.clone();
    controller.connect_key_pressed(move |_, keyval, _keycode, _mods| {
        if keyval == gtk::gdk::Key::Escape {
            s.input(RemarkedInput::Hide);
            glib::Propagation::Stop
        } else if keyval == gtk::gdk::Key::Right {
            s.input(RemarkedInput::ScrollToPage(Page::Notes));
            glib::Propagation::Stop
        } else if keyval == gtk::gdk::Key::Left {
            s.input(RemarkedInput::ScrollToPage(Page::Tasks));
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(controller);
}

// These helpers walk the note pane to retrieve the toggle button and revealer
// that were tagged with widget names. This avoids holding every widget in the
// model; replace with direct references if the layout changes.
fn find_note_sidebar_button(note_page: &gtk::Box) -> gtk::Button {
    find_named_widget(note_page, "note-sidebar-toggle")
        .and_downcast::<gtk::Button>()
        .expect("note sidebar toggle button")
}

fn find_note_sidebar_revealer(note_page: &gtk::Box) -> gtk::Revealer {
    find_named_widget(note_page, "note-sidebar-revealer")
        .and_downcast::<gtk::Revealer>()
        .expect("note sidebar revealer")
}

fn find_named_widget(root: &impl IsA<gtk::Widget>, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone().upcast());
    }
    if let Some(child) = root.first_child() {
        let mut current = Some(child);
        while let Some(widget) = current {
            if let Some(found) = find_named_widget(&widget, name) {
                return Some(found);
            }
            current = widget.next_sibling();
        }
    }
    None
}
