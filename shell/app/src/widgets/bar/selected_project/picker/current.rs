use super::{
    actions::{Action, Actions, icon_button},
    choices::{CheckoutChoice, ProjectChoice},
    rows::remove_button,
};
use adw::prelude::*;
use shell_core::gtk;
use std::rc::Rc;

pub(super) struct CurrentRow {
    pub workspace: u64,
    pub project_id: String,
    pub checkout_path: String,
    stack: gtk::Stack,
    display: adw::ActionRow,
    entry: adw::EntryRow,
}
impl CurrentRow {
    pub fn new(
        workspace: u64,
        project: &ProjectChoice,
        checkout: &CheckoutChoice,
        actions: &Rc<Actions>,
    ) -> Self {
        let stack = gtk::Stack::new();
        stack.set_hhomogeneous(false);
        stack.set_vhomogeneous(false);
        let display_list = gtk::ListBox::new();
        display_list.add_css_class("boxed-list");
        display_list.set_selection_mode(gtk::SelectionMode::None);
        let display = adw::ActionRow::new();
        display.set_use_markup(false);
        display.set_title_lines(1);
        display.set_subtitle_lines(1);
        let edit = icon_button("document-edit-symbolic", "Edit project name");
        display.add_suffix(&edit);
        display.add_suffix(&remove_button(project, actions));
        display_list.append(&display);
        let edit_list = gtk::ListBox::new();
        edit_list.add_css_class("boxed-list");
        edit_list.set_selection_mode(gtk::SelectionMode::None);
        let entry = adw::EntryRow::new();
        entry.set_title("Project name");
        entry.set_show_apply_button(true);
        let cancel = icon_button("window-close-symbolic", "Cancel project rename");
        entry.add_suffix(&cancel);
        edit_list.append(&entry);
        stack.add_named(&display_list, Some("display"));
        stack.add_named(&edit_list, Some("edit"));
        let switch = stack.downgrade();
        let field = entry.downgrade();
        let title = display.downgrade();
        edit.connect_clicked(move |_| {
            if let (Some(stack), Some(entry), Some(display)) =
                (switch.upgrade(), field.upgrade(), title.upgrade())
            {
                entry.set_text(&display.title());
                stack.set_visible_child_name("edit");
                entry.grab_focus();
            }
        });
        let switch = stack.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(stack) = switch.upgrade() {
                stack.set_visible_child_name("display");
            }
        });
        let weak = Rc::downgrade(actions);
        let id = project.id.clone();
        let switch = stack.downgrade();
        entry.connect_apply(move |entry| {
            let name = entry.text().trim().to_owned();
            if name.is_empty() {
                entry.add_css_class("error");
                return;
            }
            entry.remove_css_class("error");
            let Some(actions) = weak.upgrade() else {
                return;
            };
            entry.set_sensitive(false);
            let field = entry.downgrade();
            let switch = switch.clone();
            actions.run_with_completion(
                Action::Rename {
                    id: id.clone(),
                    name,
                },
                move |success| {
                    if let Some(entry) = field.upgrade() {
                        entry.set_sensitive(true);
                    }
                    if success {
                        if let Some(stack) = switch.upgrade() {
                            stack.set_visible_child_name("display");
                        }
                    }
                },
            );
        });
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let switch = stack.downgrade();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                if let Some(stack) = switch.upgrade() {
                    stack.set_visible_child_name("display");
                }
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
        entry.add_controller(keys);
        let row = Self {
            workspace,
            project_id: project.id.clone(),
            checkout_path: checkout.path.clone(),
            stack,
            display,
            entry,
        };
        row.update(project, checkout);
        row
    }
    pub fn widget(&self) -> &gtk::Stack {
        &self.stack
    }
    pub fn update(&self, project: &ProjectChoice, checkout: &CheckoutChoice) {
        self.display.set_title(&project.name);
        let count = project.checkouts.len();
        self.display.set_subtitle(&format!(
            "{} · {} · {} checkout{}",
            checkout.branch,
            checkout.path,
            count,
            if count == 1 { "" } else { "s" }
        ));
        if self.stack.visible_child_name().as_deref() != Some("edit") {
            self.entry.set_text(&project.name);
        }
    }
}
