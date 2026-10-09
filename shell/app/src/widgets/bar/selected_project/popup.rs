use super::{
    ProjectCatalog, ProjectInitializer, SelectedWorkspaceView, picker::ProjectPicker,
    rename::WorkspaceEditor,
};
use shell_core::{
    gtk::{self, prelude::*},
    gtk4_layer_shell::{KeyboardMode, LayerShell},
};
use std::{cell::RefCell, rc::Rc};

pub(in crate::widgets::bar) struct WorkspacePopup {
    target: Rc<RefCell<SelectedWorkspaceView>>,
    popover: gtk::Popover,
    rename: WorkspaceEditor,
    picker: ProjectPicker,
    assign: gtk::Button,
    unassign: gtk::Button,
    stack: adw::ViewStack,
}

impl WorkspacePopup {
    pub(in crate::widgets::bar) fn new(
        button: &gtk::MenuButton,
        initializer: ProjectInitializer,
    ) -> Self {
        let popover = gtk::Popover::new();
        let overlay = adw::ToastOverlay::new();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        let stack = adw::ViewStack::new();
        stack.set_hhomogeneous(false);
        stack.set_vhomogeneous(false);
        let switcher = adw::InlineViewSwitcher::new();
        switcher.set_stack(Some(&stack));
        switcher.set_can_shrink(true);
        switcher.set_display_mode(adw::InlineViewSwitcherDisplayMode::Labels);
        switcher.set_halign(gtk::Align::Center);
        switcher.set_valign(gtk::Align::Start);
        let workspace_page = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let rename = WorkspaceEditor::new(&popover);
        workspace_page.append(rename.widget());
        let assign = gtk::Button::new();
        assign.set_child(Some(
            &adw::ButtonContent::builder()
                .icon_name("folder-new-symbolic")
                .label("Assign project…")
                .build(),
        ));
        workspace_page.append(&assign);
        stack.add_titled(&workspace_page, Some("workspace"), "Workspace");
        let picker = ProjectPicker::new(&popover, initializer.clone(), &overlay);
        let unassign = gtk::Button::new();
        unassign.set_child(Some(
            &adw::ButtonContent::builder()
                .icon_name("list-remove-symbolic")
                .label("Unassign project from workspace")
                .build(),
        ));
        workspace_page.append(&unassign);
        let project_picker = picker.clone();
        unassign.connect_clicked(move |_| project_picker.unassign_current());
        stack.add_titled(picker.widget(), Some("project"), "Project");
        content.append(&stack);
        content.append(&switcher);
        overlay.set_child(Some(&content));
        popover.set_child(Some(&overlay));
        button.set_popover(Some(&popover));
        let target = Rc::new(RefCell::new(SelectedWorkspaceView::default()));
        let project_picker = picker.clone();
        let project_stack = stack.downgrade();
        assign.connect_clicked(move |_| {
            if let Some(stack) = project_stack.upgrade() {
                stack.set_visible_child_name("project");
                project_picker.open_selector();
            }
        });
        let workspace_editor = rename.clone();
        let project_picker = picker.clone();
        let opened = popover.downgrade();
        stack.connect_visible_child_name_notify(move |stack| {
            let project = stack.visible_child_name().as_deref() == Some("project");
            if opened.upgrade().is_some_and(|p| p.is_visible()) {
                if project {
                    project_picker.prepare();
                } else {
                    workspace_editor.prepare();
                }
            }
        });
        let stack_for_open = stack.clone();
        let workspace_editor = rename.clone();
        let project_picker = picker.clone();
        popover.connect_visible_notify(move |popover| {
            if let Some(window) = popover.root().and_downcast::<gtk::Window>() {
                window.set_keyboard_mode(if popover.is_visible() {
                    KeyboardMode::Exclusive
                } else {
                    KeyboardMode::None
                });
            }
            if popover.is_visible() {
                if stack_for_open.visible_child_name().as_deref() == Some("project") {
                    project_picker.prepare();
                } else {
                    workspace_editor.prepare();
                }
            }
        });
        Self {
            target,
            popover,
            rename,
            picker,
            assign,
            unassign,
            stack,
        }
    }

    pub(in crate::widgets::bar) fn set_catalog(&self, catalog: &ProjectCatalog) {
        self.picker.set_catalog(catalog);
    }

    pub(in crate::widgets::bar) fn set_target(&self, view: &SelectedWorkspaceView) {
        let previous = self.target.borrow().clone();
        if previous.workspace_id != view.workspace_id {
            self.popover.popdown();
        }
        *self.target.borrow_mut() = view.clone();
        let assigned = view.project.is_some();
        self.assign.set_visible(!assigned);
        self.unassign.set_visible(assigned);
        self.rename.set_target(view);
        self.picker.set_target(view);
        if previous.workspace_id != view.workspace_id || previous.project.is_some() != assigned {
            self.stack
                .set_visible_child_name(if assigned { "project" } else { "workspace" });
        }
    }
}
