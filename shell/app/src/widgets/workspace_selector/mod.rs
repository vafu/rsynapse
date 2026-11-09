//! Centered, keyboard-first workspace/project selector.
use std::{cell::Cell, collections::HashMap, rc::Rc};

use adw::prelude::*;
use relm4::prelude::*;
use shell_core::{
    gtk,
    source::StateSignal,
    window::{self, Layer, WindowConfig},
};

mod actions;
mod controller;
mod list;
mod rows;
mod source;

use rows::{EntryInit, EntryKey, EntryState};

#[derive(Debug)]
pub(crate) enum WorkspaceSelectorInput {
    Source(sources::Msg),
    Show,
    Hide,
    Toggle,
    QueryChanged(String),
    RowSelected(Option<String>),
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
    search: Option<gtk::SearchEntry>,
    list: Option<gtk::ListBox>,
    scrolled: Option<gtk::ScrolledWindow>,
    error: Option<gtk::Label>,
    visible: bool,
    query: String,
    entries: Vec<EntryInit>,
    row_states: HashMap<EntryKey, StateSignal<Option<EntryState>>>,
    selected_key: StateSignal<Option<EntryKey>>,
    busy: bool,
    dialog_open: Rc<Cell<bool>>,
    suppress_selection: Rc<Cell<bool>>,

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

            #[wrap(Some)]
            set_content = &gtk::Box {
                add_css_class: "workspace-selector-frame",
                set_orientation: gtk::Orientation::Vertical,

                gtk::Box {
                    add_css_class: "workspace-selector",
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 10,
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    set_width_request: 560,

                    gtk::Box {
                        add_css_class: "workspace-selector-header",
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 2,

                        gtk::Label {
                            add_css_class: "workspace-selector-title",
                            set_xalign: 0.0,
                            #[watch]
                            set_label: header_title(&model.view).as_str(),
                        },

                        gtk::Label {
                            add_css_class: "workspace-selector-subtitle",
                            set_xalign: 0.0,
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                            set_max_width_chars: 72,
                            #[watch]
                            set_label: header_subtitle(&model.view).as_str(),
                        },
                    },

                    #[name = "search"]
                    gtk::SearchEntry {
                        set_placeholder_text: Some("Search projects or type a workspace name"),
                        set_hexpand: true,
                    },

                    #[name = "scrolled"]
                    gtk::ScrolledWindow {
                        set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                        set_propagate_natural_height: true,
                        set_max_content_height: 380,

                        #[name = "list"]
                        gtk::ListBox {
                            add_css_class: "boxed-list",
                            add_css_class: "workspace-selector-list",
                            set_selection_mode: gtk::SelectionMode::Single,
                            set_activate_on_single_click: true,
                        },
                    },

                    #[name = "empty"]
                    gtk::Label {
                        add_css_class: "dim-label",
                        set_label: "No projects registered",
                        #[watch]
                        set_visible: model.entries.is_empty(),
                    },

                    #[name = "error"]
                    gtk::Label {
                        add_css_class: "error",
                        set_wrap: true,
                        set_xalign: 0.0,
                        set_visible: false,
                    },

                    gtk::Label {
                        add_css_class: "dim-label",
                        add_css_class: "workspace-selector-hint",
                        set_label: "↑↓ choose · Enter apply · Del remove · Esc close",
                        set_xalign: 0.0,
                    },
                },
            },
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

        let dialog_open = Rc::new(Cell::new(false));
        let suppress_selection = Rc::new(Cell::new(false));
        let mut model = WorkspaceSelector::new(
            root.clone(),
            None,
            None,
            None,
            None,
            false,
            String::new(),
            Vec::new(),
            HashMap::new(),
            StateSignal::new(None),
            false,
            dialog_open.clone(),
            suppress_selection.clone(),
        );
        let widgets = view_output!();
        model.search = Some(widgets.search.clone());
        model.list = Some(widgets.list.clone());
        model.scrolled = Some(widgets.scrolled.clone());
        model.error = Some(widgets.error.clone());

        widgets
            .search
            .update_property(&[gtk::accessible::Property::Label(
                "Search projects or workspace names",
            )]);

        let input = sender.input_sender().clone();
        widgets.search.connect_changed(move |search| {
            let _ = input.send(WorkspaceSelectorInput::QueryChanged(
                search.text().to_string(),
            ));
        });

        let input = sender.input_sender().clone();
        widgets.list.connect_row_activated(move |_, _| {
            let _ = input.send(WorkspaceSelectorInput::Activate);
        });

        let input = sender.input_sender().clone();
        let suppress = suppress_selection.clone();
        widgets.list.connect_row_selected(move |_, row| {
            if suppress.get() {
                return;
            }
            let key = row
                .filter(|row| !rows::row_identity(row).is_empty())
                .map(rows::row_identity);
            let _ = input.send(WorkspaceSelectorInput::RowSelected(key));
        });

        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let input = sender.input_sender().clone();
        let key_dialog_open = dialog_open.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
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
                self.selected_key.set(None);
                self.refresh_entries();
            }
            WorkspaceSelectorInput::RowSelected(key) => {
                self.selected_key.set(key.and_then(|name| {
                    self.entries
                        .iter()
                        .find(|entry| entry.key().row_name() == name)
                        .map(EntryInit::key)
                }));
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

fn header_title(view: &source::View) -> String {
    view.workspace_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("Workspace")
        .to_owned()
}

fn header_subtitle(view: &source::View) -> String {
    match (&view.current_project_name, &view.current_project_id) {
        (Some(name), _) => format!("Current project · {name}"),
        (None, Some(_)) => "Current project".to_owned(),
        (None, None) => "No project assigned".to_owned(),
    }
}
