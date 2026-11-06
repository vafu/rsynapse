//! Centered, keyboard-first workspace/project selector.
use std::{cell::Cell, rc::Rc};

use adw::prelude::*;
use relm4::prelude::*;
use shell_core::{
    gtk,
    source::{ListDiff, StateSignal},
    window::{self, Layer, WindowConfig},
};

mod actions;
mod controller;
mod list;
mod rows;
mod source;

use rows::Entry;

#[derive(Debug)]
pub(crate) enum WorkspaceSelectorInput {
    Source(sources::Msg),
    Show,
    Hide,
    Toggle,
    QueryChanged(String),
    MoveSelection(i32),
    Activate,
    DeleteSelected,
    RemoveProject { id: String },
    RemoveResult(Result<(), String>),
    ApplyResult(Result<(), String>),
}

impl From<sources::Msg> for WorkspaceSelectorInput {
    fn from(msg: sources::Msg) -> Self {
        Self::Source(msg)
    }
}

#[shell_macros::model]
pub(crate) struct WorkspaceSelector {
    window: adw::ApplicationWindow,
    search: gtk::SearchEntry,
    list: gtk::ListBox,
    scrolled: gtk::ScrolledWindow,
    header_title: gtk::Label,
    header_subtitle: gtk::Label,
    empty: gtk::Label,
    error: gtk::Label,
    hint: gtk::Label,
    visible: bool,
    query: String,
    entries: Vec<Entry>,
    entries_signal: StateSignal<Vec<Entry>>,
    busy: bool,
    dialog_open: Rc<Cell<bool>>,
    selection_reset: bool,

    #[source(shell_core::source::diff_by_key(entries_signal.observable(), Entry::key))]
    list_diff: ListDiff<Entry>,

    #[source(source::view())]
    view: source::View,
}

#[shell_macros::component(model = WorkspaceSelector)]
#[relm4::component(pub(crate))]
impl SimpleComponent for WorkspaceSelector {
    type Init = ();
    type Input = WorkspaceSelectorInput;
    type Output = ();

    view! {
        #[root]
        adw::ApplicationWindow {
            add_css_class: "workspace-selector-window",
            set_title: Some("Workspace Selector"),
            set_decorated: false,
            set_hide_on_close: true,
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        window::apply_layer_shell_config(
            &root,
            WindowConfig::new(Layer::Overlay).with_namespace("rsynapse-workspace-selector"),
        );
        root.set_decorated(false);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
        content.add_css_class("workspace-selector");
        content.set_halign(gtk::Align::Center);
        content.set_valign(gtk::Align::Center);
        content.set_width_request(560);

        let header = gtk::Box::new(gtk::Orientation::Vertical, 2);
        header.add_css_class("workspace-selector-header");
        let header_title = gtk::Label::new(Some("Workspace"));
        header_title.add_css_class("workspace-selector-title");
        header_title.set_xalign(0.0);
        let header_subtitle = gtk::Label::new(None);
        header_subtitle.add_css_class("workspace-selector-subtitle");
        header_subtitle.set_xalign(0.0);
        header_subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
        header_subtitle.set_max_width_chars(72);
        header.append(&header_title);
        header.append(&header_subtitle);
        content.append(&header);

        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some("Search projects or type a workspace name"));
        search.set_hexpand(true);
        search.update_property(&[gtk::accessible::Property::Label(
            "Search projects or workspace names",
        )]);
        content.append(&search);

        let list = gtk::ListBox::new();
        list.add_css_class("boxed-list");
        list.add_css_class("workspace-selector-list");
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.set_activate_on_single_click(true);

        let scrolled = gtk::ScrolledWindow::new();
        scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scrolled.set_propagate_natural_height(true);
        scrolled.set_max_content_height(380);
        scrolled.set_child(Some(&list));
        content.append(&scrolled);

        let empty = gtk::Label::new(Some("No projects registered"));
        empty.add_css_class("dim-label");
        empty.set_visible(false);
        content.append(&empty);

        let error = gtk::Label::new(None);
        error.add_css_class("error");
        error.set_wrap(true);
        error.set_xalign(0.0);
        error.set_visible(false);
        content.append(&error);

        let hint = gtk::Label::new(Some("↑↓ choose · Enter apply · Del remove · Esc close"));
        hint.add_css_class("dim-label");
        hint.add_css_class("workspace-selector-hint");
        hint.set_xalign(0.0);
        content.append(&hint);

        let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
        frame.add_css_class("workspace-selector-frame");
        frame.append(&content);
        root.set_content(Some(&frame));

        let input = sender.input_sender().clone();
        search.connect_changed(move |search| {
            let _ = input.send(WorkspaceSelectorInput::QueryChanged(
                search.text().to_string(),
            ));
        });

        let input = sender.input_sender().clone();
        list.connect_row_activated(move |_, _| {
            let _ = input.send(WorkspaceSelectorInput::Activate);
        });

        let dialog_open = Rc::new(Cell::new(false));
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let input = sender.input_sender().clone();
        let key_dialog_open = dialog_open.clone();
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            use gtk::gdk::Key;

            if key_dialog_open.get() {
                return gtk::glib::Propagation::Proceed;
            }

            match key {
                Key::Escape => {
                    let _ = input.send(WorkspaceSelectorInput::Hide);
                    gtk::glib::Propagation::Stop
                }
                Key::Up | Key::KP_Up => {
                    let _ = input.send(WorkspaceSelectorInput::MoveSelection(-1));
                    gtk::glib::Propagation::Stop
                }
                Key::Down | Key::KP_Down => {
                    let _ = input.send(WorkspaceSelectorInput::MoveSelection(1));
                    gtk::glib::Propagation::Stop
                }
                Key::Tab => {
                    let delta = if modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
                        -1
                    } else {
                        1
                    };
                    let _ = input.send(WorkspaceSelectorInput::MoveSelection(delta));
                    gtk::glib::Propagation::Stop
                }
                Key::ISO_Left_Tab => {
                    let _ = input.send(WorkspaceSelectorInput::MoveSelection(-1));
                    gtk::glib::Propagation::Stop
                }
                Key::Return | Key::KP_Enter => {
                    let _ = input.send(WorkspaceSelectorInput::Activate);
                    gtk::glib::Propagation::Stop
                }
                Key::Delete | Key::KP_Delete => {
                    let _ = input.send(WorkspaceSelectorInput::DeleteSelected);
                    gtk::glib::Propagation::Stop
                }
                _ => gtk::glib::Propagation::Proceed,
            }
        });
        root.add_controller(keys);

        let close_sender = sender.clone();
        root.connect_close_request(move |_| {
            close_sender.input(WorkspaceSelectorInput::Hide);
            gtk::glib::Propagation::Stop
        });

        let mut model = Self::new(
            root.clone(),
            search,
            list,
            scrolled,
            header_title,
            header_subtitle,
            empty,
            error,
            hint,
            false,
            String::new(),
            Vec::new(),
            StateSignal::new(Vec::new()),
            false,
            dialog_open,
            false,
        );
        let widgets = view_output!();
        model.refresh_entries();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            WorkspaceSelectorInput::Source(message) => {
                Self::update(self, message);
                if self.changed(sources::Field::View) {
                    self.refresh_entries();
                }
                if self.changed(sources::Field::ListDiff) {
                    self.apply_list_diff();
                }
                self.clear_changed();
            }
            WorkspaceSelectorInput::Show => self.show(),
            WorkspaceSelectorInput::Hide => self.hide(),
            WorkspaceSelectorInput::Toggle => {
                if self.visible {
                    self.hide();
                } else {
                    self.show();
                }
            }
            WorkspaceSelectorInput::QueryChanged(query) => {
                self.query = query;
                self.selection_reset = true;
                if !self.refresh_entries() {
                    self.selection_reset = false;
                    self.select_default();
                }
            }
            WorkspaceSelectorInput::MoveSelection(delta) => self.move_selection(delta),
            WorkspaceSelectorInput::Activate => self.activate(&sender),
            WorkspaceSelectorInput::DeleteSelected => self.delete_selected(&sender),
            WorkspaceSelectorInput::RemoveProject { id } => self.remove_project(id, &sender),
            WorkspaceSelectorInput::RemoveResult(result) => self.remove_result(result),
            WorkspaceSelectorInput::ApplyResult(result) => self.apply_result(result),
        }
    }
}
