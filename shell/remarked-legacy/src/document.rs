use super::{WorkspaceInput, WorkspaceWindow, client::NotesClient};
use adw::prelude::*;
use notebook_api::note::{NoteBody, NoteId};
use relm4::ComponentSender;

impl WorkspaceWindow {
    pub(super) fn clear_editor(&mut self) {
        self.state.read_version += 1;
        self.state.reading = false;
        self.state.read_again = false;
        self.state.selected_note = None;
        self.state.loaded = None;
        self.state.original_tags.clear();
        self.state.new_note = false;
        self.state.dirty = false;
    }

    pub(super) fn load(&mut self, id: NoteId, sender: &ComponentSender<Self>) {
        let Some(path) = self.notebook_path() else {
            return;
        };
        if self.state.reading {
            self.state.read_again = true;
            return;
        }
        self.state.reading = true;
        self.state.read_version += 1;
        let version = self.state.read_version;
        let tx = sender.input_sender().clone();
        relm4::spawn(async move {
            let result =
                async { NotesClient::open(&path, Vec::new()).await?.body(&id).await }.await;
            tx.emit(WorkspaceInput::Loaded {
                path,
                id,
                version,
                result,
            });
        });
    }

    pub(super) fn loaded(
        &mut self,
        path: String,
        id: NoteId,
        version: u64,
        result: Result<NoteBody, String>,
        sender: &ComponentSender<Self>,
    ) {
        if self.notebook_path().as_deref() != Some(&path)
            || self.state.selected_note.as_ref() != Some(&id)
            || self.state.read_version != version
        {
            return;
        }
        self.state.reading = false;
        match result {
            Ok(body) if !self.state.dirty && !self.state.busy => {
                let mut tags = body.tags.clone();
                tags.sort();
                tags.dedup();
                self.ui.rendering.set(true);
                // Keep cursor and undo history on unchanged refreshes, but never
                // carry undo history from a different note or external revision.
                if self
                    .state
                    .loaded
                    .as_ref()
                    .is_none_or(|old| old.text != body.text)
                {
                    let buffer = self.ui.editor.buffer();
                    buffer.set_enable_undo(false);
                    buffer.set_text(&body.text);
                    buffer.set_enable_undo(true);
                }
                self.ui.tags.set_text(&tags.join(", "));
                self.ui.rendering.set(false);
                self.state.original_tags = tags;
                self.state.loaded = Some(body);
                self.ui.navigation.set_show_content(true);
            }
            Ok(_) => {}
            Err(error) => self.show_result(Err(error)),
        }
        if self.state.read_again && !self.state.dirty && !self.state.busy {
            self.state.read_again = false;
            if let Some(id) = self.state.selected_note.clone() {
                self.load(id, sender);
            }
        }
    }
}
