use super::{WorkspaceInput, WorkspaceWindow, client::TaskAction, model::Navigate};
use adw::prelude::*;
use relm4::ComponentSender;

impl WorkspaceWindow {
    pub(super) fn request_navigation(
        &mut self,
        target: Navigate,
        sender: &ComponentSender<Self>,
        root: &adw::ApplicationWindow,
    ) {
        if self.state.busy {
            return;
        }
        if self.state.dirty {
            let dialog = adw::AlertDialog::builder()
                .heading("Save this note?")
                .body("Your draft stays with this project and task until you save or discard it.")
                .build();
            dialog.add_responses(&[
                ("cancel", "Keep editing"),
                ("discard", "Discard"),
                ("save", "Save"),
            ]);
            dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
            dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
            dialog.set_default_response(Some("save"));
            dialog.set_close_response("cancel");
            let tx = sender.input_sender().clone();
            dialog.connect_response(None, move |_, response| {
                tx.emit(WorkspaceInput::Prompt {
                    response: response.into(),
                    target: target.clone(),
                })
            });
            dialog.present(Some(root));
        } else {
            self.navigate(target, sender, root);
        }
    }

    pub(super) fn task_dialog(
        &self,
        subtask: bool,
        edit: bool,
        sender: &ComponentSender<Self>,
        root: &adw::ApplicationWindow,
    ) {
        let Some(project) = self.selected_project().cloned() else {
            return;
        };
        let task = self.selected_task();
        if (subtask || edit) && task.is_none() {
            return;
        }
        let dialog = adw::AlertDialog::builder()
            .heading(if edit {
                "Edit task"
            } else if subtask {
                "New subtask"
            } else {
                "New task"
            })
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let title = gtk::Entry::builder()
            .placeholder_text("Task title")
            .activates_default(true)
            .build();
        let description = gtk::Entry::builder()
            .placeholder_text("Outcome or description")
            .build();
        if edit {
            if let Some(task) = task {
                title.set_text(&task.goal.title);
                description.set_text(&task.goal.description);
            }
        }
        content.append(&title);
        content.append(&description);
        dialog.set_extra_child(Some(&content));
        dialog.add_responses(&[
            ("cancel", "Cancel"),
            ("save", if edit { "Save" } else { "Create" }),
        ]);
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));
        dialog.set_close_response("cancel");
        let key = task.map(|task| task.key.clone()).unwrap_or_default();
        let tx = sender.input_sender().clone();
        dialog.connect_response(None, move |_, response| {
            let name = title.text().trim().to_owned();
            if response != "save" || name.is_empty() {
                return;
            }
            let action = if edit {
                TaskAction::Edit {
                    key: key.clone(),
                    title: name,
                    description: description.text().to_string(),
                }
            } else {
                TaskAction::Create {
                    project: project.clone(),
                    parent: if subtask { key.clone() } else { String::new() },
                    title: name,
                    description: description.text().to_string(),
                }
            };
            tx.emit(WorkspaceInput::TaskAction(action));
        });
        dialog.present(Some(root));
    }
}
