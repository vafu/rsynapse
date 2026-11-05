//! Lifecycle and keyboard operations for the workspace selector.
use adw::prelude::*;
use relm4::prelude::*;
use shell_core::{
    gtk,
    gtk4_layer_shell::{KeyboardMode, LayerShell},
};

use super::{
    WorkspaceSelector, WorkspaceSelectorInput, actions,
    rows::{self, Entry},
};

impl WorkspaceSelector {
    pub(super) fn show(&mut self) {
        self.visible = true;
        self.busy = false;
        self.error.set_text("");
        self.error.set_visible(false);
        if !self.query.is_empty() {
            self.query.clear();
            self.search.set_text("");
        }
        self.selection_reset = true;
        if !self.refresh_entries() {
            self.selection_reset = false;
            self.select_default();
        }
        self.window.set_keyboard_mode(KeyboardMode::Exclusive);
        self.window.present();
        let search = self.search.clone();
        gtk::glib::idle_add_local_once(move || {
            search.grab_focus();
        });
    }

    pub(super) fn hide(&mut self) {
        self.visible = false;
        self.window.set_keyboard_mode(KeyboardMode::None);
        self.window.set_visible(false);
    }

    pub(super) fn refresh_entries(&mut self) -> bool {
        let title = self
            .view
            .workspace_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or("Workspace");
        self.header_title.set_text(title);

        let subtitle = match (
            &self.view.current_project_name,
            &self.view.current_project_id,
        ) {
            (Some(name), _) => format!("Current project · {name}"),
            (None, Some(_)) => "Current project".to_owned(),
            (None, None) => "No project assigned".to_owned(),
        };
        self.header_subtitle.set_text(&subtitle);

        let entries = rows::entries(&self.view, &self.query);
        let changed = self.entries_signal.get() != entries;
        self.entries_signal.set(entries);
        changed
    }

    pub(super) fn activate(&mut self, sender: &ComponentSender<Self>) {
        if self.busy {
            return;
        }
        let Some(index) = self.list.selected_row().map(|row| row.index()) else {
            return;
        };
        let Some(entry) = self.entries.get(index as usize).cloned() else {
            return;
        };
        let Some(workspace) = self.view.workspace_id.clone() else {
            self.set_error("No focused workspace is available.");
            return;
        };

        match entry {
            Entry::Project { current: true, .. } => self.hide(),
            Entry::Project { path, .. } => {
                self.busy = true;
                let input = sender.input_sender().clone();
                relm4::spawn(async move {
                    let result = actions::assign(workspace, path)
                        .await
                        .map_err(|error| error.to_string());
                    let _ = input.send(WorkspaceSelectorInput::ApplyResult(result));
                });
            }
            Entry::Unassign => {
                self.busy = true;
                let input = sender.input_sender().clone();
                relm4::spawn(async move {
                    let result = actions::unassign(workspace)
                        .await
                        .map_err(|error| error.to_string());
                    let _ = input.send(WorkspaceSelectorInput::ApplyResult(result));
                });
            }
            Entry::Rename { name } => {
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
        let Some(index) = self.list.selected_row().map(|row| row.index()) else {
            return;
        };
        let Some(Entry::Project { id, name, .. }) = self.entries.get(index as usize).cloned()
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
            Ok(()) => self.error.set_visible(false),
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
        self.error.set_text(message);
        self.error.set_visible(true);
    }
}
