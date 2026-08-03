use std::{cell::RefCell, fmt, rc::Rc};

use gtk::{self, prelude::*};

use crate::{NerdIcon, search_icons};

const MAX_SEARCH_RESULTS: usize = 24;
const ICONS_PER_ROW: u32 = 6;

type SelectionHandler = Box<dyn Fn(NerdIcon)>;
type ResetHandler = Box<dyn Fn()>;

/// A reusable GTK widget with specific icons and searchable Nerd Font results.
#[derive(Clone)]
pub struct NerdIconPicker {
    root: gtk::Box,
    state: Rc<PickerState>,
}

struct PickerState {
    search: gtk::SearchEntry,
    specific_flow: gtk::FlowBox,
    results_flow: gtk::FlowBox,
    results_status: gtk::Label,
    reset_button: gtk::Button,
    specific_icons: RefCell<Vec<NerdIcon>>,
    selected_glyph: RefCell<Option<String>>,
    selection_handlers: RefCell<Vec<SelectionHandler>>,
    reset_handlers: RefCell<Vec<ResetHandler>>,
}

impl NerdIconPicker {
    /// Construct an empty picker. Consumers can supply specific icons later.
    pub fn new() -> Self {
        let root = gtk::Box::builder()
            .css_classes(["nerd-icon-picker"])
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .build();
        let search = gtk::SearchEntry::builder()
            .placeholder_text("Search Nerd Font icons")
            .build();
        root.append(&search);

        root.append(&section_label("Specific icons"));
        let specific_flow = icon_flow();
        root.append(&specific_flow);

        let reset_button = gtk::Button::builder()
            .css_classes(["flat", "nerd-icon-picker-reset"])
            .label("Use automatic icon")
            .halign(gtk::Align::Start)
            .visible(false)
            .build();
        root.append(&reset_button);

        root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        root.append(&section_label("Search results"));

        let results_flow = icon_flow();
        root.append(&results_flow);
        let results_status = gtk::Label::builder()
            .css_classes(["dim-label", "nerd-icon-picker-status"])
            .label("Type a name to search the Nerd Font catalog")
            .halign(gtk::Align::Start)
            .wrap(true)
            .build();
        root.append(&results_status);

        let state = Rc::new(PickerState {
            search,
            specific_flow,
            results_flow,
            results_status,
            reset_button,
            specific_icons: RefCell::new(Vec::new()),
            selected_glyph: RefCell::new(None),
            selection_handlers: RefCell::new(Vec::new()),
            reset_handlers: RefCell::new(Vec::new()),
        });
        connect_signals(&state);

        Self { root, state }
    }

    /// Return the widget root for embedding in a GTK layout.
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    /// Replace the current search text and refresh the result grid.
    pub fn set_query(&self, query: &str) {
        self.state.search.set_text(query);
    }

    /// Move keyboard focus to the search entry.
    pub fn focus_search(&self) {
        self.state.search.grab_focus();
    }

    /// Replace the icons displayed in the dedicated specific-icons row.
    pub fn set_specific_icons(&self, icons: Vec<NerdIcon>) {
        *self.state.specific_icons.borrow_mut() = icons;
        refresh_specific_icons(&self.state);
    }

    /// Mark a glyph as the current selection.
    pub fn set_selected_glyph(&self, glyph: Option<&str>) {
        *self.state.selected_glyph.borrow_mut() = glyph.map(str::to_owned);
        refresh_specific_icons(&self.state);
        refresh_results(&self.state);
    }

    /// Show or hide the action that restores automatic icon selection.
    pub fn set_reset_visible(&self, visible: bool) {
        self.state.reset_button.set_visible(visible);
    }

    /// Run a callback whenever the user chooses an icon.
    pub fn connect_icon_selected(&self, handler: impl Fn(NerdIcon) + 'static) {
        self.state
            .selection_handlers
            .borrow_mut()
            .push(Box::new(handler));
    }

    /// Run a callback when the user asks to restore automatic selection.
    pub fn connect_reset(&self, handler: impl Fn() + 'static) {
        self.state
            .reset_handlers
            .borrow_mut()
            .push(Box::new(handler));
    }
}

impl Default for NerdIconPicker {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for NerdIconPicker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NerdIconPicker")
            .field("specific_icons", &self.state.specific_icons.borrow().len())
            .field("selected_glyph", &self.state.selected_glyph.borrow())
            .finish_non_exhaustive()
    }
}

fn connect_signals(state: &Rc<PickerState>) {
    let weak_state = Rc::downgrade(state);
    state.search.connect_search_changed(move |_| {
        if let Some(state) = weak_state.upgrade() {
            refresh_results(&state);
        }
    });

    let weak_state = Rc::downgrade(state);
    state.reset_button.connect_clicked(move |_| {
        let Some(state) = weak_state.upgrade() else {
            return;
        };
        for handler in state.reset_handlers.borrow().iter() {
            handler();
        }
    });
}

fn refresh_specific_icons(state: &Rc<PickerState>) {
    state.specific_flow.remove_all();
    for icon in state.specific_icons.borrow().iter().cloned() {
        state
            .specific_flow
            .append(&icon_button(state, icon, "specific"));
    }
}

fn refresh_results(state: &Rc<PickerState>) {
    state.results_flow.remove_all();
    let query = state.search.text();
    let results = search_icons(query.as_str(), MAX_SEARCH_RESULTS);
    for icon in results.iter().cloned() {
        state
            .results_flow
            .append(&icon_button(state, icon, "result"));
    }

    state.results_status.set_visible(results.is_empty());
    state.results_status.set_label(if query.trim().is_empty() {
        "Type a name to search the Nerd Font catalog"
    } else {
        "No matching Nerd Font icons"
    });
}

fn icon_button(state: &Rc<PickerState>, icon: NerdIcon, kind: &str) -> gtk::Button {
    let selected = state.selected_glyph.borrow().as_deref() == Some(icon.glyph());
    let button = gtk::Button::builder()
        .css_classes(["flat", "nerd-icon-picker-choice", kind])
        .tooltip_text(icon.name())
        .build();
    if selected {
        button.add_css_class("selected");
    }

    let label = gtk::Label::builder()
        .css_classes(["nerdicon", "nerd-icon-picker-glyph"])
        .label(icon.glyph())
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    button.set_child(Some(&label));

    let weak_state = Rc::downgrade(state);
    button.connect_clicked(move |_| {
        let Some(state) = weak_state.upgrade() else {
            return;
        };
        for handler in state.selection_handlers.borrow().iter() {
            handler(icon.clone());
        }
    });
    button
}

fn icon_flow() -> gtk::FlowBox {
    gtk::FlowBox::builder()
        .column_spacing(4)
        .row_spacing(4)
        .homogeneous(true)
        .max_children_per_line(ICONS_PER_ROW)
        .min_children_per_line(ICONS_PER_ROW)
        .selection_mode(gtk::SelectionMode::None)
        .build()
}

fn section_label(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .css_classes(["heading", "nerd-icon-picker-section-label"])
        .label(text)
        .halign(gtk::Align::Start)
        .build()
}
