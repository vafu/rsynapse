use super::{ProjectInitializer, SelectedWorkspaceView};
use shell_core::gtk::{self, prelude::*};
use std::{cell::RefCell, rc::Rc};
mod actions;
mod choices;
mod current;
mod rows;
use actions::{Action, Actions};
pub(in crate::widgets::bar) use choices::{ProjectCatalog, project_catalog};
use current::CurrentRow;

#[derive(Clone)]
pub(super) struct ProjectPicker {
    state: Rc<PickerState>,
}
struct PickerState {
    target: RefCell<SelectedWorkspaceView>,
    catalog: RefCell<ProjectCatalog>,
    current: RefCell<Option<CurrentRow>>,
    content: gtk::Box,
    summary: gtk::Box,
    no_project: gtk::Label,
    chooser_popover: gtk::Popover,
    search: gtk::SearchEntry,
    results: gtk::ListBox,
    empty: gtk::Label,
    actions: Rc<Actions>,
}
impl ProjectPicker {
    pub(super) fn new(
        popover: &gtk::Popover,
        initializer: ProjectInitializer,
        overlay: &adw::ToastOverlay,
    ) -> Self {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let summary = gtk::Box::new(gtk::Orientation::Vertical, 0);
        summary.set_visible(false);
        content.append(&summary);
        let no_project = gtk::Label::new(Some("No project assigned"));
        no_project.add_css_class("dim-label");
        content.append(&no_project);
        let choose_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let chooser = gtk::MenuButton::new();
        chooser.set_hexpand(true);
        chooser.set_child(Some(
            &adw::ButtonContent::builder()
                .label("Choose project")
                .icon_name("folder-open-symbolic")
                .build(),
        ));
        choose_row.append(&chooser);
        content.append(&choose_row);
        let chooser_popover = gtk::Popover::new();
        let panel = gtk::Box::new(gtk::Orientation::Vertical, 8);
        panel.set_margin_top(12);
        panel.set_margin_bottom(12);
        panel.set_margin_start(12);
        panel.set_margin_end(12);
        let search_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let search = gtk::SearchEntry::new();
        search.set_hexpand(true);
        search.set_placeholder_text(Some("Search projects, checkouts or branches"));
        search.update_property(&[gtk::accessible::Property::Label("Search projects")]);
        search_row.append(&search);
        panel.append(&search_row);
        let results = gtk::ListBox::new();
        results.add_css_class("boxed-list");
        results.set_selection_mode(gtk::SelectionMode::None);
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_propagate_natural_height(true);
        scroll.set_child(Some(&results));
        panel.append(&scroll);
        let empty = gtk::Label::new(Some("No matching projects"));
        empty.add_css_class("dim-label");
        panel.append(&empty);
        let add = gtk::Button::new();
        add.set_child(Some(
            &adw::ButtonContent::builder()
                .icon_name("folder-open-symbolic")
                .label("Choose project from directory…")
                .build(),
        ));
        panel.append(&add);
        chooser_popover.set_child(Some(&panel));
        chooser.set_popover(Some(&chooser_popover));
        let state = Rc::new(PickerState {
            target: RefCell::new(SelectedWorkspaceView::default()),
            catalog: RefCell::new(ProjectCatalog::default()),
            current: RefCell::new(None),
            content,
            summary,
            no_project,
            chooser_popover: chooser_popover.clone(),
            search,
            results,
            empty,
            actions: Actions::new(overlay),
        });
        let weak = Rc::downgrade(&state);
        state.search.connect_changed(move |_| {
            if let Some(s) = weak.upgrade() {
                s.rebuild_results();
            }
        });
        let weak = Rc::downgrade(&state);
        chooser_popover.connect_visible_notify(move |popup| {
            if !popup.is_visible() {
                return;
            }
            if let Some(s) = weak.upgrade() {
                if let Some(window) = popup.root().and_downcast::<gtk::Window>() {
                    if let Some(surface) = window.surface() {
                        if let Some(monitor) = surface.display().monitor_at_surface(&surface) {
                            scroll.set_max_content_height(monitor.geometry().height() / 3);
                        }
                    }
                }
                s.search.set_text("");
                s.rebuild_results();
                let search = s.search.clone();
                gtk::glib::idle_add_local_once(move || {
                    search.grab_focus();
                });
            }
        });
        let weak = Rc::downgrade(&state);
        let close = popover.downgrade();
        add.connect_clicked(move |_| {
            if let Some(s) = weak.upgrade() {
                let view = s.target.borrow().clone();
                if let Some(workspace) = view.workspace_id {
                    s.chooser_popover.popdown();
                    if let Some(p) = close.upgrade() {
                        p.popdown();
                    }
                    initializer.open_binding(
                        workspace,
                        super::title_label(&view),
                        view.project.is_none(),
                    );
                }
            }
        });
        Self { state }
    }
    pub(super) fn widget(&self) -> &gtk::Box {
        &self.state.content
    }
    pub(super) fn set_catalog(&self, catalog: &ProjectCatalog) {
        if *self.state.catalog.borrow() != *catalog {
            *self.state.catalog.borrow_mut() = catalog.clone();
            self.state.rebuild_current();
            if self.state.chooser_popover.is_visible() {
                self.state.rebuild_results();
            }
        }
    }
    pub(super) fn set_target(&self, view: &SelectedWorkspaceView) {
        *self.state.target.borrow_mut() = view.clone();
        self.state.rebuild_current();
    }
    pub(super) fn prepare(&self) {
        self.state.rebuild_current();
    }
    pub(super) fn selector_is_open(&self) -> bool {
        self.state.chooser_popover.is_visible()
    }
    pub(super) fn open_selector(&self) {
        let popup = self.state.chooser_popover.clone();
        gtk::glib::idle_add_local_once(move || popup.popup());
    }
    pub(super) fn unassign_current(&self) {
        let workspace = self.state.target.borrow().workspace_id;
        if let Some(workspace) = workspace {
            self.state.actions.run(Action::Unassign(workspace));
        }
    }
}
impl PickerState {
    fn rebuild_current(self: &Rc<Self>) {
        let project_id = self
            .target
            .borrow()
            .project
            .as_ref()
            .and_then(|p| p.project_id.clone());
        let workspace = self.target.borrow().workspace_id;
        let current = workspace.and_then(|id| {
            self.catalog
                .borrow()
                .active
                .iter()
                .find(|p| Some(&p.id) == project_id.as_ref())
                .and_then(|p| p.checkouts.first().map(|c| (id, p.clone(), c.clone())))
        });
        self.summary.set_visible(current.is_some());
        self.no_project.set_visible(current.is_none());
        if let Some((workspace, project, checkout)) = &current {
            if let Some(row) = self.current.borrow().as_ref() {
                if row.workspace == *workspace
                    && row.project_id == project.id
                    && row.checkout_path == checkout.path
                {
                    row.update(project, checkout);
                    return;
                }
            }
        }
        self.current.borrow_mut().take();
        while let Some(child) = self.summary.first_child() {
            self.summary.remove(&child);
        }
        if let Some((workspace, project, checkout)) = current {
            let row = CurrentRow::new(workspace, &project, &checkout, &self.actions);
            self.summary.append(row.widget());
            *self.current.borrow_mut() = Some(row);
        }
    }
    fn rebuild_results(self: &Rc<Self>) {
        while let Some(child) = self.results.first_child() {
            self.results.remove(&child);
        }
        let query = self.search.text();
        let catalog = self.catalog.borrow().clone();
        let projects = catalog.active;
        let mut count = 0;
        for project in projects.into_iter().filter(|p| p.matches(&query)) {
            rows::catalog_row(&project, self, &query);
            count += 1;
        }
        self.empty.set_text("No matching projects");
        self.empty.set_visible(count == 0);
    }
}
