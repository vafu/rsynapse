use super::super::SelectedWorkspaceView;
use super::actions::{Actions, ScopedAction};
use shell_core::{
    gtk::{self, prelude::*},
    source::Observable,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

mod model;
mod state;
#[cfg(test)]
mod test;
mod watch;

use model::Target;

pub(super) struct LabelEditor(Rc<State>);

struct State {
    content: gtk::Box,
    list: gtk::ListBox,
    status: gtk::Label,
    entry: gtk::Entry,
    add: gtk::Button,
    actions: Rc<Actions>,
    target: RefCell<Option<Target>>,
    generation: Cell<u64>,
    labels: RefCell<Option<Vec<String>>>,
    busy: Cell<bool>,
    watch: RefCell<Option<watch::Watch>>,
    command: RefCell<Option<ScopedAction>>,
}

impl LabelEditor {
    pub(super) fn new(actions: &Rc<Actions>) -> Self {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        content.set_visible(false);
        let heading = gtk::Label::new(Some("Labels"));
        heading.set_xalign(0.0);
        heading.add_css_class("heading");
        content.append(&heading);
        let explanation = gtk::Label::new(Some(
            "GitHub PRs match ANY label. With no labels, all PRs are shown.",
        ));
        explanation.set_xalign(0.0);
        explanation.set_wrap(true);
        explanation.set_max_width_chars(44);
        explanation.add_css_class("dim-label");
        content.append(&explanation);
        let list = gtk::ListBox::new();
        list.add_css_class("boxed-list");
        list.set_selection_mode(gtk::SelectionMode::None);
        content.append(&list);
        let status = gtk::Label::new(None);
        status.set_xalign(0.0);
        status.add_css_class("dim-label");
        content.append(&status);
        let input = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let entry = gtk::Entry::new();
        entry.set_hexpand(true);
        entry.set_placeholder_text(Some("Add a label"));
        entry.update_property(&[gtk::accessible::Property::Label("Project label")]);
        input.append(&entry);
        let add = gtk::Button::with_label("Add");
        input.append(&add);
        content.append(&input);
        let state = Rc::new(State {
            content,
            list,
            status,
            entry,
            add,
            actions: actions.clone(),
            target: RefCell::new(None),
            generation: Cell::new(0),
            labels: RefCell::new(None),
            busy: Cell::new(false),
            watch: RefCell::new(None),
            command: RefCell::new(None),
        });
        let weak = Rc::downgrade(&state);
        state.entry.connect_changed(move |_| {
            if let Some(state) = weak.upgrade() {
                state.update_controls();
            }
        });
        let weak = Rc::downgrade(&state);
        state.entry.connect_activate(move |_| {
            if let Some(state) = weak.upgrade() {
                state.add_label();
            }
        });
        let weak = Rc::downgrade(&state);
        state.add.connect_clicked(move |_| {
            if let Some(state) = weak.upgrade() {
                state.add_label();
            }
        });
        state.update_controls();
        Self(state)
    }

    pub(super) fn widget(&self) -> &gtk::Box {
        &self.0.content
    }

    pub(super) fn set_target(&self, view: &SelectedWorkspaceView) {
        self.retarget(view, shell_core::source::proj::project_labels);
    }

    fn retarget(
        &self,
        view: &SelectedWorkspaceView,
        source: impl FnOnce(String) -> Observable<Option<Vec<String>>>,
    ) {
        let project = view.project.as_ref().and_then(|p| p.project_id.clone());
        if self
            .0
            .target
            .borrow()
            .as_ref()
            .map(|t| (&t.workspace, &t.project))
            == project.as_ref().map(|p| (&view.workspace_id, p))
        {
            return;
        }
        let generation = self.0.generation.get().wrapping_add(1);
        self.0.generation.set(generation);
        self.0.watch.borrow_mut().take();
        self.0.command.borrow_mut().take();
        self.0.busy.set(false);
        let target = project.map(|project| Target {
            workspace: view.workspace_id.clone(),
            project,
            generation,
        });
        *self.0.target.borrow_mut() = target.clone();
        *self.0.labels.borrow_mut() = None;
        self.0.entry.set_text("");
        self.0.content.set_visible(target.is_some());
        self.0.render();
        if let Some(target) = target {
            let source = source(target.project.clone());
            *self.0.watch.borrow_mut() = Some(watch::Watch::new(&self.0, target, source));
        }
    }
}
