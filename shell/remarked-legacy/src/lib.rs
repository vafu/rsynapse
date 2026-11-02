//! Rsynapse's task-and-notes surface. Projd owns tasks; Remarked owns notes.
//! This component is hosted by the existing shell application, not a UI daemon.
mod client;
mod dialogs;
mod document;
mod layout;
mod message;
mod model;
mod operations;
mod overlay;
mod render;
mod source;
#[cfg(test)]
mod test;

use adw::prelude::*;
use model::{Navigate, State};
use notebook_api::note::NoteBody;
use relm4::prelude::*;
use shell_core::source::{self as sources_api, StateSignal};
use source::{Catalog, Notebook, Notes};

pub use message::WorkspaceInput;

#[shell_macros::model]
pub struct WorkspaceWindow {
    ui: layout::Layout,
    state: State,
    notebook: StateSignal<Option<Notebook>>,
    #[source(source::catalog())]
    catalog: Catalog,
    #[source(sources_api::switch_map(notebook.observable(), source::notes))]
    notes: Notes,
}

#[shell_macros::component(model = WorkspaceWindow)]
#[relm4::component(pub)]
impl SimpleComponent for WorkspaceWindow {
    type Init = Option<String>;
    type Input = WorkspaceInput;
    type Output = ();

    view! {
        #[root]
        adw::ApplicationWindow {
            add_css_class: "workspace-window",
            set_title: Some("Tasks & Notes — Rsynapse"),
            set_default_width: 1180,
            set_default_height: 780,
            set_hide_on_close: true,
            set_visible: false,
        }
    }

    fn init(
        project: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        overlay::configure(&root);
        let ui = layout::Layout::new(&root, sender.input_sender().clone());
        let state = State {
            project,
            ..Default::default()
        };
        let model = Self::new(ui, state, StateSignal::new(None));
        let widgets = view_output!();
        model.render();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        let window = self.ui.window.clone();
        let root = &window;
        match message {
            WorkspaceInput::Source(message) => {
                Self::update(self, message);
                self.reconcile(&sender);
            }
            WorkspaceInput::Open(project) => {
                overlay::show(root);
                if let Some(project) = project {
                    if self.state.project.as_ref() != Some(&project) {
                        self.request_navigation(Navigate::Project(project), &sender, root);
                    }
                }
            }
            WorkspaceInput::Hide => self.request_navigation(Navigate::Close, &sender, root),
            WorkspaceInput::Toggle(project) => {
                sender.input(if root.is_visible() {
                    WorkspaceInput::Hide
                } else {
                    WorkspaceInput::Open(project)
                });
            }
            WorkspaceInput::Navigate(target) => self.request_navigation(target, &sender, root),
            WorkspaceInput::ProjectIndex(index) => {
                if let Catalog::Ready { projects, .. } = &self.catalog {
                    if let Some(project) = projects.get(index as usize) {
                        self.request_navigation(
                            Navigate::Project(project.id.clone()),
                            &sender,
                            root,
                        );
                    }
                }
            }
            WorkspaceInput::Prompt { response, target } => match response.as_str() {
                "discard" => {
                    self.state.dirty = false;
                    self.navigate(target, &sender, root);
                }
                "save" => {
                    self.state.pending = Some(target);
                    self.save(&sender);
                }
                _ => {}
            },
            WorkspaceInput::Filter(query) => self.state.query = query,
            WorkspaceInput::Changed => {
                let buffer = self.ui.editor.buffer();
                let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
                self.state.dirty = self.state.new_note
                    || self.state.loaded.as_ref().is_some_and(|loaded| {
                        loaded.text != text
                            || self.state.original_tags != model::parse_tags(&self.ui.tags.text())
                    });
                self.render_controls();
                return;
            }
            WorkspaceInput::Save => self.save(&sender),
            WorkspaceInput::Reload => {
                if let Some(id) = self.state.selected_note.clone() {
                    self.request_navigation(Navigate::Note(id), &sender, root);
                }
            }
            WorkspaceInput::Retry => self.set_notebook(true),
            WorkspaceInput::Start => {
                if let Some(task) = self.selected_task() {
                    sender.input(WorkspaceInput::TaskAction(client::TaskAction::Start {
                        key: task.key.clone(),
                        project: task.goal.project_id.clone(),
                    }));
                }
            }
            WorkspaceInput::StopTracking => {
                if let Some(project) = &self.state.project {
                    sender.input(WorkspaceInput::TaskAction(client::TaskAction::Clear {
                        project: project.clone(),
                    }));
                }
            }
            WorkspaceInput::Status(index) => {
                if let (Some(task), Some(status)) = (
                    self.selected_task(),
                    [
                        "planned",
                        "in-progress",
                        "completed",
                        "deferred",
                        "cancelled",
                    ]
                    .get(index as usize),
                ) {
                    sender.input(WorkspaceInput::TaskAction(client::TaskAction::Status {
                        key: task.key.clone(),
                        status: (*status).into(),
                    }));
                }
            }
            WorkspaceInput::TaskDialog { subtask, edit } => {
                self.task_dialog(subtask, edit, &sender, root)
            }
            WorkspaceInput::TaskAction(action) => {
                if !self.state.busy {
                    self.state.busy = true;
                    let tx = sender.input_sender().clone();
                    relm4::spawn(async move {
                        tx.emit(WorkspaceInput::TaskDone(client::task_action(action).await));
                    });
                }
            }
            WorkspaceInput::TaskDone(result) => {
                self.state.busy = false;
                self.show_result(result.map(|_| "Task updated"));
            }
            WorkspaceInput::Import => self.import(&sender),
            WorkspaceInput::Loaded {
                path,
                id,
                version,
                result,
            } => self.loaded(path, id, version, result, &sender),
            WorkspaceInput::Saved {
                path,
                id,
                text,
                tags,
                result,
            } => {
                if self.notebook_path().as_deref() == Some(&path)
                    && self.state.selected_note.as_ref() == Some(&id)
                {
                    self.state.busy = false;
                    match result {
                        Ok(patch) => {
                            self.state.loaded = Some(NoteBody {
                                note_id: id,
                                text,
                                revision: patch.revision,
                                tags: tags.clone(),
                                ..Default::default()
                            });
                            self.state.original_tags = tags;
                            self.state.dirty = false;
                            self.show_result(Ok("Note saved"));
                            if let Some(target) = self.state.pending.take() {
                                self.navigate(target, &sender, root);
                            }
                        }
                        Err(error) => {
                            self.state.pending = None;
                            self.show_result(Err(error));
                        }
                    }
                }
            }
            WorkspaceInput::Created { path, result } => {
                if self.notebook_path().as_deref() == Some(&path) {
                    self.state.busy = false;
                    match result {
                        Ok(body) => {
                            let mut tags = body.tags.clone();
                            tags.sort();
                            tags.dedup();
                            self.ui.rendering.set(true);
                            let buffer = self.ui.editor.buffer();
                            buffer.set_enable_undo(false);
                            buffer.set_text(&body.text);
                            buffer.set_enable_undo(true);
                            self.ui.tags.set_text(&tags.join(", "));
                            self.ui.rendering.set(false);
                            self.ui.navigation.set_show_content(true);
                            self.state.selected_note = Some(body.note_id.clone());
                            self.state.original_tags = tags;
                            self.state.loaded = Some(body);
                            self.state.new_note = false;
                            self.state.dirty = false;
                            self.show_result(Ok("Note created"));
                            if let Some(target) = self.state.pending.take() {
                                self.navigate(target, &sender, root);
                            }
                        }
                        Err(error) => {
                            self.state.pending = None;
                            self.show_result(Err(error));
                        }
                    }
                }
            }
            WorkspaceInput::Render => {}
        }
        self.render();
    }
}
