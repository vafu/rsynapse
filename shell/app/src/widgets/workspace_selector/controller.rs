//! Lifecycle and keyboard operations for the workspace selector.
use adw::prelude::*;
use relm4::prelude::*;
use shell_core::{
    gtk,
    gtk4_layer_shell::{KeyboardMode, LayerShell},
};

use super::{WorkspaceSelector, WorkspaceSelectorInput, actions, rows::EntryState};

impl WorkspaceSelector {
    pub(super) fn show(&mut self) {
        self.visible = true;
        self.busy = false;
        self.set_error_text(None);
        if !self.query.is_empty() {
            self.query.clear();
            if let Some(search) = self.search.as_ref() {
                search.set_text("");
            }
        }
        self.selected_key.set(None);
        self.refresh_entries();
        if let Some(search) = self.search.as_ref() {
            let search = search.clone();
            gtk::glib::idle_add_local_once(move || {
                search.grab_focus();
            });
        }
        self.window.set_keyboard_mode(KeyboardMode::Exclusive);
        self.window.present();
    }

    pub(super) fn hide(&mut self) {
        self.visible = false;
        self.window.set_keyboard_mode(KeyboardMode::None);
        self.window.set_visible(false);
    }

    pub(super) fn activate(&mut self, sender: &ComponentSender<Self>) {
        if self.busy {
            return;
        }
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let Some(workspace) = self.view.workspace_id.clone() else {
            self.set_error("No focused workspace is available.");
            return;
        };

        match entry {
            EntryState::Project { current: true, .. } => self.hide(),
            EntryState::Project { path, .. } => {
                self.busy = true;
                let input = sender.input_sender().clone();
                relm4::spawn(async move {
                    let result = actions::assign(workspace, path)
                        .await
                        .map_err(|error| error.to_string());
                    let _ = input.send(WorkspaceSelectorInput::ApplyResult(result));
                });
            }
            EntryState::Unassign => {
                self.busy = true;
                let input = sender.input_sender().clone();
                relm4::spawn(async move {
                    let result = actions::unassign(workspace)
                        .await
                        .map_err(|error| error.to_string());
                    let _ = input.send(WorkspaceSelectorInput::ApplyResult(result));
                });
            }
            EntryState::Rename { name } => {
                let name = name.trim().to_owned();
                if name.is_empty() {
                    self.set_error("Enter a workspace name.");
                    return;
                }
                if self.view.workspace_name.as_deref().map(str::trim) == Some(name.as_str()) {
                    self.hide();
                    return;
                }
                self.busy = true;
                let input = sender.input_sender().clone();
                relm4::spawn(async move {
                    let result = actions::rename(workspace, name)
                        .await
                        .map_err(|error| error.to_string());
                    let _ = input.send(WorkspaceSelectorInput::ApplyResult(result));
                });
            }
        }
    }

    pub(super) fn delete_selected(&mut self, sender: &ComponentSender<Self>) {
        let Some(id) = self.selected_project_id() else {
            return;
        };
        let Some(name) = self
            .entries
            .iter()
            .find(|entry| entry.key() == super::rows::EntryKey::Project(id.clone()))
            .and_then(|entry| entry.state().get().as_ref().map(EntryState::title))
        else {
            return;
        };
        self.confirm_remove(id, name, sender);
    }

    pub(super) fn confirm_remove(
        &mut self,
        id: String,
        name: String,
        sender: &ComponentSender<Self>,
    ) {
        let dialog = adw::AlertDialog::builder()
            .heading("Remove project?")
            .body(format!(
                "Delete project metadata for “{name}” and its checkout? Project files and directories are not touched."
            ))
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("remove", "Remove");
        dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        self.dialog_open.set(true);
        let open = self.dialog_open.clone();
        dialog.connect_closed(move |_| open.set(false));
        let input = sender.input_sender().clone();
        dialog.connect_response(None, move |_, response| {
            if response == "remove" {
                let _ = input.send(WorkspaceSelectorInput::RemoveProject { id: id.clone() });
            }
        });
        dialog.present(Some(&self.window));
    }

    pub(super) fn remove_project(&mut self, id: String, sender: &ComponentSender<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        let input = sender.input_sender().clone();
        relm4::spawn(async move {
            let result = actions::remove_project(id)
                .await
                .map_err(|error| error.to_string());
            let _ = input.send(WorkspaceSelectorInput::RemoveResult(result));
        });
    }

    pub(super) fn remove_result(&mut self, result: Result<(), String>) {
        self.busy = false;
        match result {
            Ok(()) => self.set_error_text(None),
            Err(error) => self.set_error(&error),
        }
    }

    pub(super) fn apply_result(&mut self, result: Result<(), String>) {
        self.busy = false;
        match result {
            Ok(()) => self.hide(),
            Err(error) => self.set_error(&error),
        }
    }

    fn set_error(&mut self, message: &str) {
        self.set_error_text(Some(message));
    }

    fn set_error_text(&mut self, message: Option<&str>) {
        if let Some(error) = self.error.as_ref() {
            error.set_text(message.unwrap_or_default());
            error.set_visible(message.is_some());
        }
    }
}
