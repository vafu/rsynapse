//! Rsynapse's shell-like task-and-notes overlay (v2).
use adw::prelude::*;
use gtk::{gdk, glib, prelude::WidgetExt};
use relm4::prelude::*;
use shell_core::{
    gtk4_layer_shell::{KeyboardMode, LayerShell},
    window::{Anchors, Layer, WindowConfig},
};

/// Input messages for the remarked overlay.
#[derive(Debug, Clone)]
pub enum RemarkedInput {
    Show,
    Hide,
    Toggle,
    ScrollToPage(Page),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Tasks,
    Notes,
}

/// A transparent fullscreen layer-shell overlay for tasks and notes.
pub struct RemarkedWindow {
    window: adw::ApplicationWindow,
    carousel: adw::Carousel,
    task_page: gtk::Box,
    note_page: gtk::Box,
    task_revealer: gtk::Revealer,
    task_chip: gtk::Box,
    note_sidebar_button: gtk::Button,
    note_sidebar_revealer: gtk::Revealer,
    selected_task: Option<String>,
    selected_note: Option<String>,
    visible: bool,
}

impl RemarkedWindow {
    fn new(
        window: adw::ApplicationWindow,
        carousel: adw::Carousel,
        task_page: gtk::Box,
        note_page: gtk::Box,
        task_revealer: gtk::Revealer,
        task_chip: gtk::Box,
        note_sidebar_button: gtk::Button,
        note_sidebar_revealer: gtk::Revealer,
    ) -> Self {
        Self {
            window,
            carousel,
            task_page,
            note_page,
            task_revealer,
            task_chip,
            note_sidebar_button,
            note_sidebar_revealer,
            selected_task: None,
            selected_note: None,
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

    fn scroll_to(&self, page: Page) {
        let child = match page {
            Page::Tasks => &self.task_page,
            Page::Notes => &self.note_page,
        };
        self.carousel.scroll_to(child, true);
    }

    fn set_task(&mut self, task: Option<String>) {
        self.selected_task = task.clone();
        self.task_chip.set_visible(task.is_some());
        self.task_revealer.set_reveal_child(false);
    }

    fn set_note(&mut self, note: Option<String>) {
        self.selected_note = note.clone();
        let has_note = note.is_some();
        self.note_sidebar_button.set_sensitive(has_note);
        if !has_note {
            self.note_sidebar_revealer.set_reveal_child(false);
        }
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

        let carousel = adw::Carousel::builder()
            .allow_scroll_wheel(false)
            .allow_mouse_drag(false)
            .build();

        let task_page = build_task_pane(&sender);
        let note_page = build_note_pane(&sender);

        carousel.append(&task_page);
        carousel.append(&note_page);

        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&carousel));

        let project_chip = chip("Project");
        project_chip.set_halign(gtk::Align::Start);
        project_chip.set_valign(gtk::Align::Start);
        project_chip.set_margin_top(24);
        project_chip.set_margin_start(24);
        overlay.add_overlay(&project_chip);

        let (task_chip, task_revealer) = task_chip(&sender);
        let task_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Start)
            .margin_top(24)
            .build();
        task_box.append(&task_chip);
        task_box.append(&task_revealer);
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
            note_sidebar_button,
            note_sidebar_revealer,
        );
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
            RemarkedInput::ScrollToPage(page) => self.scroll_to(page),
        }
    }
}

fn build_task_pane(_sender: &ComponentSender<RemarkedWindow>) -> gtk::Box {
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
    for i in 1..=5 {
        let row = adw::ActionRow::builder()
            .title(&format!("Task {i}"))
            .subtitle("placeholder task")
            .activatable(true)
            .build();
        list_box.append(&row);
    }

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
    details.add(&adw::ActionRow::builder().title("Status").subtitle("planned").build());
    details.add(&adw::ActionRow::builder().title("Priority").subtitle("medium").build());
    details.add(&adw::ActionRow::builder().title("Description").subtitle("...").build());

    split.set_content(Some(&content));
    split.set_sidebar(Some(&details));

    let split_for_button = split.clone();
    details_button.connect_clicked(move |_| {
        split_for_button.set_show_sidebar(!split_for_button.shows_sidebar());
    });

    let page = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    page.append(&split);
    page
}

fn build_note_pane(_sender: &ComponentSender<RemarkedWindow>) -> gtk::Box {
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
    let clear_button = gtk::Button::builder()
        .label("Clear filter")
        .css_classes(["flat"])
        .visible(false)
        .build();
    list_header.append(&title);
    list_header.append(&clear_button);

    let list_box = gtk::ListBox::builder()
        .css_classes(["boxed-list"])
        .selection_mode(gtk::SelectionMode::Single)
        .vexpand(true)
        .build();
    for i in 1..=5 {
        let row = adw::ActionRow::builder()
            .title(&format!("Note {i}"))
            .subtitle("placeholder note")
            .activatable(true)
            .build();
        list_box.append(&row);
    }

    let sidebar = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .width_request(280)
        .build();
    sidebar.append(&list_header);
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
    note_details.add(&adw::ActionRow::builder().title("Backlinks").subtitle("...").build());
    note_details.add(&adw::ActionRow::builder().title("Tags").subtitle("...").build());
    note_details.add(&adw::ActionRow::builder().title("Created").subtitle("...").build());
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
        sidebar_revealer_for_toggle
            .set_reveal_child(!sidebar_revealer_for_toggle.reveals_child());
    });

    // Store references so the model can find them later.
    toggle.set_widget_name("note-sidebar-toggle");
    sidebar_revealer.set_widget_name("note-sidebar-revealer");

    let page = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    page.append(&split);
    page
}

fn chip(label: &str) -> gtk::Box {
    let chip = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .css_classes(["remarked-chip"])
        .build();
    let label = gtk::Label::builder().label(label).build();
    chip.append(&label);
    chip
}

fn task_chip(_sender: &ComponentSender<RemarkedWindow>) -> (gtk::Box, gtk::Revealer) {
    let chip = gtk::Button::builder()
        .label("Selected task")
        .css_classes(["remarked-chip"])
        .build();

    let revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .build();
    let details = adw::PreferencesGroup::builder()
        .title("Task metadata")
        .build();
    details.add(&adw::ActionRow::builder().title("Status").subtitle("planned").build());
    details.add(&adw::ActionRow::builder().title("Priority").subtitle("medium").build());
    details.add(&adw::ActionRow::builder().title("Description").subtitle("...").build());
    revealer.set_child(Some(&details));

    let r = revealer.clone();
    chip.connect_clicked(move |_| {
        r.set_reveal_child(!r.reveals_child());
    });

    let chip_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(["remarked-chip"])
        .build();
    chip_box.append(&chip);

    (chip_box, revealer)
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
        ",
    );
    gtk::style_context_add_provider_for_display(
        &WidgetExt::display(window),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn add_keyboard_handlers(window: &adw::ApplicationWindow, sender: &ComponentSender<RemarkedWindow>) {
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
