use super::{
    WorkspaceInput, WorkspaceWindow,
    client::NotesClient,
    model::{self, Navigate},
    source::{Catalog, Notebook, Notes},
};
use adw::prelude::*;
use relm4::ComponentSender;

impl WorkspaceWindow {
    pub(super) fn notebook_path(&self) -> Option<String> {
        self.selected_project()
            .map(|project| {
                std::path::Path::new(&project.cwd)
                    .join(".notes")
                    .to_string_lossy()
                    .into_owned()
            })
            .or_else(|| {
                // A temporarily unavailable/removed project cannot retarget an open
                // draft. Keep its notebook address until the editor is left.
                (self.state.loaded.is_some() || self.state.new_note)
                    .then(|| self.notebook.get().map(|notebook| notebook.path))
                    .flatten()
            })
    }

    pub(super) fn set_notebook(&mut self, force: bool) {
        let path = self.notebook_path();
        let current = self.notebook.get();
        if force || current.as_ref().map(|v| &v.path) != path.as_ref() {
            self.state.generation += 1;
            self.notebook.set(path.map(|path| Notebook {
                path,
                generation: self.state.generation,
            }));
        }
    }

    pub(super) fn reconcile(&mut self, sender: &ComponentSender<Self>) {
        if let Catalog::Ready { projects, .. } = &self.catalog {
            if !self.state.dirty
                && !projects
                    .iter()
                    .any(|p| Some(&p.id) == self.state.project.as_ref())
            {
                self.state.project = projects.first().map(|p| p.id.clone());
                self.clear_editor();
                self.state.task = None;
                self.state.initial_selection = false;
            }
        }
        if !self.state.initial_selection {
            if let Catalog::Ready { tasks, active, .. } = &self.catalog {
                self.state.task = tasks
                    .iter()
                    .find(|task| {
                        active.contains(&task.instance_id)
                            && Some(&task.goal.project_id) == self.state.project.as_ref()
                    })
                    .map(|task| task.instance_id.clone());
                self.state.initial_selection = true;
            }
        }
        self.set_notebook(false);
        if !self.state.dirty && !self.state.busy {
            if let (Some(id), Notes::Ready(path, notes)) = (&self.state.selected_note, &self.notes)
            {
                if self.notebook_path().as_ref() == Some(path)
                    && notes.iter().any(|note| note.id == *id)
                {
                    self.load(id.clone(), sender);
                }
            }
        }
    }

    pub(super) fn navigate(
        &mut self,
        target: Navigate,
        sender: &ComponentSender<Self>,
        root: &adw::ApplicationWindow,
    ) {
        match target {
            Navigate::Close => {
                super::overlay::hide(root);
            }
            Navigate::Project(id) => {
                self.clear_editor();
                self.state.project = Some(id);
                self.state.task = None;
                self.state.initial_selection = false;
                self.notes = Notes::Loading;
                self.set_notebook(false);
            }
            Navigate::Task(task) => {
                self.clear_editor();
                self.state.task = task;
                self.state.initial_selection = true;
            }
            Navigate::Note(id) => {
                self.clear_editor();
                self.state.selected_note = Some(id.clone());
                self.load(id, sender);
            }
            Navigate::New => {
                if self.selected_project().is_none() {
                    return;
                }
                self.clear_editor();
                self.state.new_note = true;
                self.state.dirty = true;
                let mut tags = vec!["kind/fleeting".into(), "status/inbox".into()];
                if let Some(task) = &self.state.task {
                    tags.push(model::task_tag(task));
                }
                tags.sort();
                self.ui.rendering.set(true);
                let buffer = self.ui.editor.buffer();
                buffer.set_enable_undo(false);
                buffer.set_text("# New note\n\n");
                buffer.set_enable_undo(true);
                self.ui.tags.set_text(&tags.join(", "));
                self.ui.rendering.set(false);
                self.ui.navigation.set_show_content(true);
                self.ui.editor.grab_focus();
            }
        }
    }

    pub(super) fn save(&mut self, sender: &ComponentSender<Self>) {
        if self.state.busy || !self.state.dirty {
            return;
        }
        let Some(path) = self.notebook_path() else {
            return;
        };
        let buffer = self.ui.editor.buffer();
        let text = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string();
        let tags = model::parse_tags(&self.ui.tags.text());
        self.state.read_version += 1;
        self.state.reading = false;
        self.state.read_again = false;
        self.state.busy = true;
        let tx = sender.input_sender().clone();
        if let Some(body) = &self.state.loaded {
            let id = body.note_id.clone();
            let revision = body.revision.clone();
            relm4::spawn(async move {
                let result = async {
                    NotesClient::open(&path, Vec::new())
                        .await?
                        .save(&id, &revision, &text, tags.clone())
                        .await
                }
                .await;
                tx.emit(WorkspaceInput::Saved {
                    path,
                    id,
                    text,
                    tags,
                    result,
                });
            });
        } else if self.state.new_note {
            relm4::spawn(async move {
                let result = async {
                    let client = NotesClient::open(&path, tags.clone()).await?;
                    let id = client.capture(&text).await?;
                    client.body(&id).await
                }
                .await;
                tx.emit(WorkspaceInput::Created { path, result });
            });
        } else {
            self.state.busy = false;
        }
    }

    pub(super) fn import(&mut self, sender: &ComponentSender<Self>) {
        if self.state.busy || self.state.dirty {
            return;
        }
        let (Some(task), Some(path)) = (self.selected_task(), self.notebook_path()) else {
            return;
        };
        let file = task.goal.note_path.clone();
        let tags = vec![
            model::task_tag(&task.instance_id),
            "source/projd-context".into(),
            "kind/fleeting".into(),
            "status/inbox".into(),
        ];
        self.state.busy = true;
        let tx = sender.input_sender().clone();
        relm4::spawn(async move {
            let result = async {
                let client =
                    NotesClient::open(&path, vec![tags[0].clone(), "source/projd-context".into()])
                        .await?;
                client.initialize().await?;
                if let Some(note) = client.list().await?.first() {
                    return client.body(&note.id).await;
                }
                let text = tokio::fs::read_to_string(file)
                    .await
                    .map_err(|e| e.to_string())?;
                let id = client.capture(&text).await?;
                client.body(&id).await
            }
            .await;
            tx.emit(WorkspaceInput::Created { path, result });
        });
    }

    pub(super) fn show_result(&self, result: Result<&str, String>) {
        match result {
            Ok(message) => {
                self.ui.banner.set_revealed(false);
                self.ui.toast.add_toast(adw::Toast::new(message));
            }
            Err(error) => {
                self.ui
                    .banner
                    .set_title(&format!("{error}. Your draft has been kept."));
                self.ui.banner.set_revealed(true);
            }
        }
    }
}
