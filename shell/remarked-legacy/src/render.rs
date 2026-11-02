use super::{
    WorkspaceWindow, model,
    source::{Catalog, Notes},
};
use adw::prelude::*;
use proj_model::{GoalIdentityInfo, ProjectInfo};

impl WorkspaceWindow {
    pub(super) fn selected_project(&self) -> Option<&ProjectInfo> {
        let Catalog::Ready { projects, .. } = &self.catalog else {
            return None;
        };
        projects
            .iter()
            .find(|project| Some(&project.id) == self.state.project.as_ref())
    }

    pub(super) fn selected_task(&self) -> Option<&GoalIdentityInfo> {
        let Catalog::Ready { tasks, .. } = &self.catalog else {
            return None;
        };
        tasks.iter().find(|task| {
            task.deleted_at_ms == 0
                && Some(&task.instance_id) == self.state.task.as_ref()
                && Some(&task.goal.project_id) == self.state.project.as_ref()
        })
    }

    pub(super) fn render(&self) {
        self.ui.rendering.set(true);
        if let Catalog::Ready { projects, .. } = &self.catalog {
            let names: Vec<_> = projects
                .iter()
                .map(|project| project.name.as_str())
                .collect();
            self.ui
                .project
                .set_model(Some(&gtk::StringList::new(&names)));
            self.ui.project.set_selected(
                projects
                    .iter()
                    .position(|p| Some(&p.id) == self.state.project.as_ref())
                    .map(|i| i as u32)
                    .unwrap_or(gtk::INVALID_LIST_POSITION),
            );
        }
        self.render_tasks();
        self.render_notes();
        self.render_controls();
        self.ui.rendering.set(false);
    }

    fn render_tasks(&self) {
        clear(&self.ui.tasks);
        match &self.catalog {
            Catalog::Loading => placeholder(&self.ui.tasks, "Loading projects…"),
            Catalog::Unavailable => placeholder(&self.ui.tasks, "Project service unavailable"),
            Catalog::Ready {
                projects,
                tasks,
                active,
            } => {
                if projects.is_empty() {
                    placeholder(&self.ui.tasks, "Register a project with proj add");
                    return;
                }
                let all = adw::ActionRow::builder()
                    .title("All project notes")
                    .subtitle("Ideas, research and decisions")
                    .activatable(true)
                    .build();
                all.set_widget_name("");
                all.add_prefix(&gtk::Image::from_icon_name("folder-documents-symbolic"));
                self.ui.tasks.append(&all);
                if self.state.task.is_none() {
                    self.ui.tasks.select_row(Some(&all));
                }
                for (task, depth) in model::task_rows(
                    tasks,
                    self.state.project.as_deref().unwrap_or(""),
                    self.ui.finished.is_active(),
                ) {
                    let row = adw::ActionRow::builder()
                        .title(glib_escape(&task.goal.title))
                        .subtitle(task.goal.status.replace('-', " "))
                        .activatable(true)
                        .build();
                    row.set_widget_name(&task.instance_id);
                    row.set_margin_start((depth.min(8) * 12) as i32);
                    row.set_tooltip_text(Some(&task.goal.description));
                    let icon = if active.contains(&task.instance_id) {
                        "find-location-symbolic"
                    } else {
                        match task.goal.status.as_str() {
                            "completed" => "emblem-ok-symbolic",
                            "cancelled" => "process-stop-symbolic",
                            "in-progress" => "media-playback-start-symbolic",
                            "deferred" => "media-playback-pause-symbolic",
                            _ => "radio-symbolic",
                        }
                    };
                    row.add_prefix(&gtk::Image::from_icon_name(icon));
                    self.ui.tasks.append(&row);
                    if self.state.task.as_ref() == Some(&task.instance_id) {
                        self.ui.tasks.select_row(Some(&row));
                    }
                }
            }
        }
    }

    pub(super) fn render_notes(&self) {
        let was_rendering = self.ui.rendering.replace(true);
        clear(&self.ui.notes);
        match &self.notes {
            Notes::Loading => placeholder(&self.ui.notes, "Opening notebook…"),
            Notes::NoProject => placeholder(&self.ui.notes, "Choose a project"),
            Notes::Error(error) => placeholder(&self.ui.notes, error),
            Notes::Ready(path, notes) => {
                if self.notebook_path().as_ref() != Some(path) {
                    placeholder(&self.ui.notes, "Opening notebook…");
                    self.ui.rendering.set(was_rendering);
                    return;
                }
                let mut notes: Vec<_> = notes
                    .iter()
                    .filter(|note| {
                        model::matches(note, self.state.task.as_deref(), &self.state.query)
                    })
                    .collect();
                notes.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.title.cmp(&b.title)));
                if notes.is_empty() {
                    placeholder(&self.ui.notes, "No notes yet. Create one with Ctrl+N.");
                }
                for note in notes {
                    let title = if note.title.is_empty() {
                        note.id.as_str()
                    } else {
                        &note.title
                    };
                    let row = adw::ActionRow::builder()
                        .title(glib_escape(title))
                        .subtitle(glib_escape(
                            &note
                                .tags
                                .iter()
                                .filter(|tag| !tag.starts_with("task/"))
                                .cloned()
                                .collect::<Vec<_>>()
                                .join(" · "),
                        ))
                        .activatable(true)
                        .build();
                    row.set_widget_name(note.id.as_str());
                    row.set_tooltip_text(Some(note.id.as_str()));
                    self.ui.notes.append(&row);
                    if self.state.selected_note.as_ref() == Some(&note.id) {
                        self.ui.notes.select_row(Some(&row));
                    }
                }
            }
        }
        self.ui.rendering.set(was_rendering);
    }

    pub(super) fn render_controls(&self) {
        let was_rendering = self.ui.rendering.replace(true);
        self.ui.project_title.set_subtitle(
            self.selected_project()
                .map(|project| project.name.as_str())
                .unwrap_or("Choose a project"),
        );
        self.ui
            .project_title
            .set_tooltip_text(self.selected_project().map(|project| project.cwd.as_str()));
        let task = self.selected_task();
        self.ui.notes_title.set_text(
            task.map(|task| task.goal.title.as_str())
                .unwrap_or("Project notes"),
        );
        self.ui.task_description.set_text(
            task.map(|task| task.goal.description.as_str())
                .unwrap_or(""),
        );
        self.ui
            .task_description
            .set_visible(task.is_some_and(|task| !task.goal.description.is_empty()));
        self.ui.start.set_visible(task.is_some());
        let is_active = task.is_some_and(|task| match &self.catalog {
            Catalog::Ready { active, .. } => active.contains(&task.instance_id),
            _ => false,
        });
        self.ui
            .start
            .set_label(if is_active { "Tracking" } else { "Start" });
        self.ui.start.set_sensitive(
            !self.state.busy && task.is_some_and(|task| !model::is_finished(&task.goal.status)),
        );
        self.ui.status.set_visible(task.is_some());
        if let Some(task) = task {
            self.ui
                .status
                .set_selected(match task.goal.status.as_str() {
                    "in-progress" => 1,
                    "completed" => 2,
                    "deferred" => 3,
                    "cancelled" => 4,
                    _ => 0,
                });
        }
        let has_import = task.is_some_and(|task| !task.goal.note_path.is_empty())
            && match &self.notes {
                Notes::Ready(path, notes) if self.notebook_path().as_ref() == Some(path) => {
                    !notes.iter().any(|note| {
                        model::matches(note, self.state.task.as_deref(), "")
                            && note.tags.iter().any(|tag| tag == "source/projd-context")
                    })
                }
                _ => false,
            };
        self.ui.import.set_visible(has_import);
        self.ui
            .import
            .set_sensitive(!self.state.busy && !self.state.dirty);
        self.ui.project.set_sensitive(!self.state.busy);
        self.ui.tasks.set_sensitive(!self.state.busy);
        self.ui.notes.set_sensitive(!self.state.busy);
        self.ui.status.set_sensitive(!self.state.busy);
        let editing = self.state.new_note || self.state.loaded.is_some();
        self.ui
            .editor_stack
            .set_visible_child_name(if editing { "editor" } else { "empty" });
        self.ui.editor.set_sensitive(!self.state.busy);
        self.ui.tags.set_sensitive(editing && !self.state.busy);
        self.ui
            .save
            .set_sensitive(editing && self.state.dirty && !self.state.busy);
        self.ui
            .reload
            .set_sensitive(self.state.loaded.is_some() && !self.state.busy);
        let name = if self.state.new_note {
            "New note"
        } else {
            self.state
                .selected_note
                .as_ref()
                .map(|id| id.as_str())
                .unwrap_or("Select a note")
        };
        self.ui.title.set_title(&format!(
            "{name}{}",
            if self.state.dirty { " •" } else { "" }
        ));
        self.ui.title.set_subtitle(if self.state.busy {
            "Working…"
        } else if self.state.dirty {
            "Unsaved changes"
        } else {
            "Markdown"
        });
        self.ui.rendering.set(was_rendering);
    }
}

fn clear(list: &gtk::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}
fn placeholder(list: &gtk::ListBox, text: &str) {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(false);
    row.set_activatable(false);
    let label = gtk::Label::new(Some(text));
    label.set_wrap(true);
    label.add_css_class("dim-label");
    label.set_margin_top(24);
    label.set_margin_bottom(24);
    label.set_margin_start(12);
    label.set_margin_end(12);
    row.set_child(Some(&label));
    list.append(&row);
}
fn glib_escape(text: &str) -> String {
    gtk::glib::markup_escape_text(text).to_string()
}
