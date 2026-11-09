//! Rsynapse's shell-like task-and-notes overlay (v2).
use adw::prelude::*;
use gtk::{gdk, glib, prelude::WidgetExt};
use relm4::prelude::*;
use shell_core::list::{ComponentListBoxExt, ComponentListUpdate};
use shell_core::{
    gtk4_layer_shell::{KeyboardMode, LayerShell},
    source::{StateSignal, workspace_project},
    window::{Layer, WindowConfig},
};
use sourceview5::prelude::*;

mod client;
mod markdown;
mod model;
mod source;
mod task_row;

use task_row::{TaskRow, TaskRowInit, TaskRowOutput};

/// Input messages for the remarked overlay.
#[derive(Debug)]
pub enum RemarkedInput {
    Source(sources::Msg),
    Show,
    Hide,
    Toggle,
    SelectTask(String),
    ClearTask,
    SelectNote(String),
    ClearNote,
    CaptureNote(String),
    SetShowFinished(bool),
    StartTask { key: String, project: String },
    StopTracking,
    SetTaskStatus { key: String, status: String },
    ToggleTaskPane(Option<bool>),
    SetEditingNote(bool),
    NoteFilter(String),
    TagsChanged,
    NoteChanged,
    SaveNote,
    ReloadNote,
    NoteLoaded(Result<notebook_api::note::NoteBody, String>),
    NoteSaved(Result<notebook_api::note::NoteMutationResult, String>),
    TaskRow(TaskRowOutput),
}

impl From<sources::Msg> for RemarkedInput {
    fn from(value: sources::Msg) -> Self {
        Self::Source(value)
    }
}

#[shell_macros::model]
pub struct RemarkedWindow {
    window: adw::ApplicationWindow,
    main_box: gtk::Box,
    task_pane: gtk::Box,
    task_stack: gtk::Stack,
    task_list: gtk::ListBox,
    task_rows: Vec<TaskRowInit>,
    task_tag_counts: StateSignal<source::TagCounts>,
    collapsed_tasks: std::rc::Rc<std::cell::RefCell<std::collections::HashSet<String>>>,
    note_pane: gtk::Box,
    note_grid: gtk::FlowBox,
    notes_stack: gtk::Stack,
    note_filter_entry: gtk::Entry,
    tasks_toggle: gtk::ToggleButton,
    note_sheet: adw::BottomSheet,
    note_title_label: gtk::Label,
    editor_box: gtk::Box,
    source_view: sourceview5::View,
    note_preview: gtk::Box,
    editor_stack: gtk::Stack,
    edit_toggle: gtk::ToggleButton,
    tags_entry: gtk::Entry,
    save_button: gtk::Button,
    show_finished_switch: gtk::Switch,
    clear_filter_button: gtk::Button,
    header_subtitle: gtk::Label,
    selected_task: StateSignal<Option<String>>,
    selected_note: StateSignal<Option<String>>,
    notebook: StateSignal<Option<source::Notebook>>,
    last_project_id: Option<String>,
    show_finished: bool,
    note_filter: String,
    note_text: String,
    original_tags: String,
    note_dirty: bool,
    note_busy: bool,
    edit_mode: bool,
    visible: bool,
    main_note_attempted: std::collections::HashSet<String>,
    #[source(source::catalog(workspace_project::focused_project_id(None)))]
    catalog: source::Catalog,
    #[source(source::notes(notebook.observable(), selected_task.observable()))]
    notes: source::Notes,
    #[source(source::tag_counts(notebook.observable()))]
    tag_counts: source::TagCounts,
}

impl RemarkedWindow {
    fn show(&mut self) {
        self.fit_to_monitor();
        self.visible = true;
        self.window.present();
        self.window.set_keyboard_mode(KeyboardMode::Exclusive);
    }

    fn hide(&mut self) {
        self.visible = false;
        self.window.set_keyboard_mode(KeyboardMode::None);
        self.window.set_visible(false);
    }

    fn selected_task_info(&self) -> Option<&proj_model::GoalIdentityInfo> {
        let id = self.selected_task.get()?;
        self.catalog.tasks().iter().find(|t| t.instance_id == id)
    }

    fn select_task(&mut self, task: Option<String>) {
        if self.selected_task.get().as_deref() != task.as_deref() {
            self.select_note(None);
            self.note_sheet.set_open(false);
        }
        self.selected_task.set(task);
        self.sync_task_selection();
    }

    fn select_note(&mut self, note: Option<String>) {
        self.selected_note.set(note);
        self.note_text.clear();
        self.original_tags.clear();
        markdown::render(&self.note_preview, "");
        self.set_editing(false);
        self.note_dirty = false;
        self.note_busy = false;
        self.note_sheet.set_open(self.selected_note.get().is_some());
    }

    fn set_note_body(&mut self, body: notebook_api::note::NoteBody) {
        let buffer = self.source_view.buffer();
        buffer.set_text(&body.text);
        markdown::render(&self.note_preview, &body.text);
        if let Some(note) = self
            .notes
            .ready_notes()
            .and_then(|notes| notes.iter().find(|note| note.id == body.note_id))
        {
            self.note_title_label.set_label(&note.title);
        }
        // NoteBody carries the document, not frontmatter. Keep the tags already
        // derived from the scoped note listing for this selected note.
        let tags = self
            .notes
            .ready_notes()
            .and_then(|notes| notes.iter().find(|note| note.id == body.note_id))
            .map(|note| note.tags.join(", "))
            .unwrap_or_default();
        self.tags_entry.set_text(&tags);
        self.original_tags = tags;
        self.note_text = body.text;
        self.note_dirty = false;
        self.note_busy = false;
    }

    fn selected_note_title(&self) -> Option<String> {
        let id = self.selected_note.get()?;
        self.notes
            .ready_notes()?
            .iter()
            .find(|note| note.id.as_str() == id)
            .map(|note| note.title.clone())
    }

    fn set_editing(&mut self, editing: bool) {
        self.edit_mode = editing;
        self.editor_stack
            .set_visible_child_name(if editing { "edit" } else { "preview" });
        self.edit_toggle.set_active(editing);
        self.save_button.set_visible(editing);
        self.update_editor_state();
    }

    fn refresh_panes(&mut self, sender: &ComponentSender<Self>) {
        if let Some(id) = self.selected_task.get() {
            if !self
                .catalog
                .tasks()
                .iter()
                .any(|task| task.instance_id == id)
            {
                self.selected_task.set(None);
            }
        }

        self.rebuild_note_grid(sender);
        self.ensure_main_note();
        self.update_editor_state();
        self.clear_changed();
    }

    fn ensure_main_note(&mut self) {
        let Some(task_id) = self.selected_task.get() else {
            return;
        };
        let Some(task) = self.selected_task_info().cloned() else {
            return;
        };
        let Some(path) = self.note_path() else {
            return;
        };
        let source::Notes::Ready {
            task: notes_task,
            notes,
            ..
        } = &self.notes
        else {
            return;
        };
        if notes_task.as_deref() != Some(task_id.as_str()) {
            return;
        }
        if notes.iter().any(|note| model::is_main_note(note, &task_id)) {
            return;
        }
        if !self.main_note_attempted.insert(task_id.clone()) {
            return;
        }
        let title = model::task_title(&task.goal.title).to_string();
        let description = task.goal.description.trim().to_string();
        let tags = vec![model::task_tag(&task_id), model::main_note_tag(&task_id)];
        relm4::spawn(async move {
            let result = async {
                let client = client::NotesClient::open(&path, tags).await?;
                let body = if description.is_empty() {
                    format!("# {title}\n")
                } else {
                    format!("# {title}\n\n{description}\n")
                };
                client.capture(&body).await.map(|_| ())
            }
            .await;
            if let Err(error) = result {
                eprintln!("[remarked] create main note failed: {error}");
            }
        });
    }

    fn reconcile(&mut self, sender: &ComponentSender<Self>) {
        self.reconcile_catalog(sender);
    }

    fn reconcile_catalog(&mut self, sender: &ComponentSender<Self>) {
        let project_id = self.catalog.project().map(|project| project.id.clone());
        if project_id != self.last_project_id {
            self.last_project_id = project_id;
            self.selected_task.set(None);
            self.select_note(None);
            self.source_view.buffer().set_text("");
            self.tags_entry.set_text("");
            self.note_filter = String::new();
            self.note_filter_entry.set_text("");
        }

        if let Some(project) = self.catalog.project() {
            self.header_subtitle.set_label(&project.name);
            let cwd = &project.cwd;
            if !cwd.is_empty() {
                self.notebook.set(Some(source::Notebook {
                    path: task_notes::notebook_path(cwd)
                        .to_string_lossy()
                        .into_owned(),
                    generation: 0,
                }));
            }
        } else {
            self.header_subtitle.set_label("No project");
            self.notebook.set(None);
        }

        self.rebuild_task_list(sender);
        self.ensure_task_selection();
        self.refresh_panes(sender);
    }

    fn ensure_task_selection(&mut self) {
        if self.selected_task.get().is_some() {
            return;
        }
        let source::Catalog::Ready { tasks, active, .. } = &self.catalog else {
            return;
        };
        let next = active
            .first()
            .cloned()
            .or_else(|| tasks.first().map(|task| task.instance_id.clone()));
        if let Some(id) = next {
            self.selected_task.set(Some(id));
        }
    }

    fn reconcile_notes(&mut self, sender: &ComponentSender<Self>) {
        self.refresh_panes(sender);
    }

    fn rebuild_task_list(&mut self, sender: &ComponentSender<Self>) {
        let tasks: &[proj_model::GoalIdentityInfo] = match &self.catalog {
            source::Catalog::Ready { tasks, .. } => tasks,
            _ => &[],
        };
        let active: &[String] = match &self.catalog {
            source::Catalog::Ready { active, .. } => active,
            _ => &[],
        };
        let nodes = model::task_nodes(tasks, self.show_finished, active);
        self.task_rows = task_row_inits(nodes, &self.collapsed_tasks, &self.task_tag_counts);
        self.task_stack
            .set_visible_child_name(if self.task_rows.is_empty() {
                "empty"
            } else {
                "list"
            });
        self.task_list.set_component_list_by_key_forward(
            ComponentListUpdate::<TaskRow>::new(&self.task_rows),
            TaskRowInit::row_key,
            sender.input_sender(),
            RemarkedInput::TaskRow,
        );
        self.sync_task_selection();
        let _ = sender;
    }

    fn sync_task_selection(&self) {
        let selected = self.selected_task.get();
        for index in 0..self.task_list.observe_children().n_items() as i32 {
            let Some(row) = self.task_list.row_at_index(index) else {
                continue;
            };
            if selected.as_deref() == Some(list_box_row_identity(&row).as_str()) {
                self.task_list.select_row(Some(&row));
                return;
            }
        }
        self.task_list.unselect_all();
    }

    fn rebuild_note_grid(&self, sender: &ComponentSender<Self>) {
        self.note_grid.remove_all();
        let selected_task = self.selected_task.get();
        let notes = match &self.notes {
            source::Notes::Ready { task, notes, .. }
                if task.as_deref() == selected_task.as_deref() =>
            {
                notes
            }
            _ => return,
        };

        let filter = self.note_filter.trim().to_lowercase();
        self.clear_filter_button
            .set_visible(selected_task.is_some());
        let mut ordered: Vec<_> = notes.iter().collect();
        if let Some(task_id) = selected_task.as_ref() {
            ordered.sort_by_key(|note| !model::is_main_note(note, task_id));
        }
        for note in ordered {
            if !filter.is_empty() {
                let hay = format!(
                    "{} {}",
                    note.title.to_lowercase(),
                    note.tags.join(" ").to_lowercase()
                );
                if !hay.contains(&filter) {
                    continue;
                }
            }
            let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
            card.add_css_class("remarked-note-card");
            card.set_size_request(240, 124);
            let title = gtk::Label::new(Some(&note.title));
            title.set_xalign(0.0);
            title.set_wrap(true);
            title.set_lines(2);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            title.add_css_class("remarked-note-card-title");
            card.append(&title);

            let kind = note.metadata.kind.to_string();
            let modified = note.modified.trim();
            let meta = if modified.is_empty() {
                kind
            } else {
                format!("{kind} · {modified}")
            };
            let meta = gtk::Label::new(Some(&meta));
            meta.set_xalign(0.0);
            meta.add_css_class("dim-label");
            meta.add_css_class("caption");
            card.append(&meta);

            let child = gtk::FlowBoxChild::new();
            child.set_child(Some(&card));
            child.set_widget_name(note.id.as_str());
            if selected_task
                .as_ref()
                .is_some_and(|id| note.tags.contains(&model::task_tag(id)))
            {
                child.add_css_class("remarked-note-active");
            }
            self.note_grid.append(&child);
        }
        self.notes_stack
            .set_visible_child_name(if notes.is_empty() { "empty" } else { "grid" });
        let _ = sender;
    }

    fn update_editor_state(&self) {
        if self.selected_note.get().is_some() {
            self.editor_box.set_visible(true);
            if let Some(title) = self.selected_note_title() {
                self.note_title_label.set_label(&title);
            }
            self.save_button
                .set_sensitive(self.edit_mode && self.note_dirty && !self.note_busy);
        } else {
            self.editor_box.set_visible(false);
        }
    }

    fn open_note(&mut self, sender: &ComponentSender<Self>, id: String) {
        self.select_note(Some(id.clone()));
        self.note_sheet.set_open(true);
        self.load_note(sender, id);
        self.update_editor_state();
    }

    fn note_path(&self) -> Option<String> {
        match &self.notes {
            source::Notes::Ready { path, .. } => Some(path.clone()),
            _ => None,
        }
    }

    fn save_note(&mut self, sender: &ComponentSender<Self>) {
        let Some(path) = self.note_path() else { return };
        let Some(id) = self.selected_note.get() else {
            return;
        };
        if self.note_busy {
            return;
        }
        let text = self
            .source_view
            .buffer()
            .text(
                &self.source_view.buffer().start_iter(),
                &self.source_view.buffer().end_iter(),
                false,
            )
            .to_string();
        let tags = model::parse_tags(&self.tags_entry.text());
        if tags != model::parse_tags(&self.original_tags) {
            eprintln!(
                "[remarked] tag edits are not persisted: the Remarked service exposes body replacement only"
            );
        }
        self.note_busy = true;
        let tx = sender.input_sender().clone();
        relm4::spawn(async move {
            let result = async {
                let client = client::NotesClient::open(&path, Vec::new()).await?;
                let body = client.body(&id.clone().into()).await?;
                client.save(&id.into(), &body.revision.into(), &text).await
            }
            .await;
            let _ = tx.send(RemarkedInput::NoteSaved(result));
        });
    }

    fn load_note(&mut self, sender: &ComponentSender<Self>, id: String) {
        let Some(path) = self.note_path() else { return };
        self.note_busy = true;
        let tx = sender.input_sender().clone();
        relm4::spawn(async move {
            let result = async {
                let client = client::NotesClient::open(&path, Vec::new()).await?;
                client.body(&id.into()).await
            }
            .await;
            let _ = tx.send(RemarkedInput::NoteLoaded(result));
        });
    }

    fn fit_to_monitor(&self) {
        let display = WidgetExt::display(&self.window);
        let monitors = display.monitors();
        let (mut width, mut height) = (2100, 1320);
        for i in 0..monitors.n_items() {
            if let Some(monitor) = monitors.item(i).and_downcast::<gdk::Monitor>() {
                let geo = monitor.geometry();
                width = width.min(geo.width() - 100);
                height = height.min(geo.height() - 100);
            }
        }
        self.main_box.set_size_request(width.max(1), height.max(1));
        self.window
            .set_default_size(width.clamp(1, i32::MAX), height.clamp(1, i32::MAX));
    }
}

#[shell_macros::component(model = RemarkedWindow)]
#[relm4::component(pub)]
impl SimpleComponent for RemarkedWindow {
    type Init = ();
    type Input = RemarkedInput;
    type Output = ();

    view! {
        #[root]
        adw::ApplicationWindow {
            add_css_class: "remarked-window",
            set_title: Some("Tasks & Notes"),
            set_hide_on_close: true,
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        shell_core::window::apply_layer_shell_config(
            &root,
            WindowConfig::new(Layer::Overlay).with_namespace("rsynapse-remarked"),
        );
        root.set_decorated(false);
        load_styles(&root);

        root.connect_close_request({
            let sender = sender.clone();
            move |_| {
                let _ = sender.input_sender().send(RemarkedInput::Hide);
                glib::Propagation::Stop
            }
        });

        let collapsed_tasks = std::rc::Rc::new(std::cell::RefCell::new(
            std::collections::HashSet::<String>::new(),
        ));
        let (task_pane, task_stack, task_list, show_finished_switch, header_subtitle) =
            build_task_pane(&sender);
        task_pane.set_hexpand(false);
        task_pane.set_width_request(360);
        task_pane.set_vexpand(true);

        let (
            note_pane,
            note_grid,
            notes_stack,
            note_filter_entry,
            clear_filter_button,
            tasks_toggle,
        ) = build_notes_surface(&sender);
        note_pane.set_hexpand(true);
        let (
            sheet_content,
            note_title_label,
            editor_box,
            source_view,
            note_preview,
            editor_stack,
            edit_toggle,
            tags_entry,
            save_button,
        ) = build_note_sheet(&sender);

        let main_surface = gtk::Box::new(gtk::Orientation::Vertical, 0);
        main_surface.set_vexpand(true);
        main_surface.set_hexpand(true);
        let body = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        body.add_css_class("remarked-workspace");
        body.set_vexpand(true);
        body.set_hexpand(true);
        body.append(&task_pane);
        body.append(&note_pane);
        main_surface.append(&body);
        let note_sheet = adw::BottomSheet::builder()
            .content(&main_surface)
            .sheet(&sheet_content)
            .show_drag_handle(true)
            .modal(true)
            .can_close(true)
            .can_open(true)
            .open(false)
            .build();
        note_sheet.add_css_class("remarked-note-sheet");
        let tx = sender.input_sender().clone();
        note_sheet.connect_close_attempt(move |sheet| {
            sheet.set_open(false);
            let _ = tx.send(RemarkedInput::ClearNote);
        });

        let main_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        main_box.add_css_class("remarked-cluster");
        main_box.append(&note_sheet);

        let frame = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .vexpand(true)
            .hexpand(true)
            .build();
        frame.add_css_class("remarked-frame");
        frame.append(&main_box);

        let clamp = adw::Clamp::builder()
            .maximum_size(2400)
            .tightening_threshold(1500)
            .vexpand(true)
            .hexpand(true)
            .child(&frame)
            .build();

        let root_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .vexpand(true)
            .hexpand(true)
            .build();
        root_box.add_css_class("remarked-root");
        root_box.append(&clamp);
        root.set_content(Some(&root_box));

        add_keyboard_handlers(&root, &sender);

        let model = Self::new(
            root.clone(),
            main_box,
            task_pane,
            task_stack,
            task_list,
            Vec::new(),
            StateSignal::new(source::TagCounts::Loading),
            collapsed_tasks,
            note_pane,
            note_grid,
            notes_stack,
            note_filter_entry,
            tasks_toggle,
            note_sheet,
            note_title_label,
            editor_box,
            source_view,
            note_preview,
            editor_stack,
            edit_toggle,
            tags_entry,
            save_button,
            show_finished_switch,
            clear_filter_button,
            header_subtitle,
            StateSignal::new(None),
            StateSignal::new(None),
            StateSignal::new(None),
            None,
            false,
            String::new(),
            String::new(),
            String::new(),
            false,
            false,
            false,
            false,
            std::collections::HashSet::new(),
        );
        let widgets = view_output!();
        model.reconcile(&sender);
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            RemarkedInput::TaskRow(output) => apply_task_row_output(self, &sender, output),
            RemarkedInput::Source(message) => {
                Self::update(self, message);
                if self.changed(sources::Field::Catalog) {
                    self.reconcile_catalog(&sender);
                } else if self.changed(sources::Field::Notes) {
                    self.reconcile_notes(&sender);
                } else if self.changed(sources::Field::TagCounts) {
                    self.task_tag_counts.set(self.tag_counts.clone());
                    self.clear_changed();
                } else {
                    self.clear_changed();
                }
            }
            RemarkedInput::Show => self.show(),
            RemarkedInput::Hide => self.hide(),
            RemarkedInput::Toggle => {
                if self.visible {
                    self.hide();
                } else {
                    self.show();
                }
            }
            RemarkedInput::SelectTask(id) => {
                self.select_task(Some(id));
                self.refresh_panes(&sender);
            }
            RemarkedInput::ClearTask => {
                self.select_task(None);
                self.refresh_panes(&sender);
            }
            RemarkedInput::SetEditingNote(editing) => {
                self.set_editing(editing);
                self.clear_changed();
            }
            RemarkedInput::SelectNote(id) => {
                self.open_note(&sender, id);
                self.clear_changed();
            }
            RemarkedInput::ToggleTaskPane(value) => {
                let show = value.unwrap_or(!self.task_pane.is_visible());
                self.task_pane.set_visible(show);
                self.tasks_toggle.set_active(show);
                self.tasks_toggle.set_icon_name(if show {
                    "view-conceal-symbolic"
                } else {
                    "view-reveal-symbolic"
                });
                self.tasks_toggle.set_active(show);
                self.clear_changed();
            }
            RemarkedInput::ClearNote => {
                self.select_note(None);
                self.update_editor_state();
                self.clear_changed();
            }
            RemarkedInput::CaptureNote(text) => {
                let text = text.trim().to_string();
                if text.is_empty() {
                    return;
                }
                if let Some(path) = self.note_path() {
                    relm4::spawn(async move {
                        match client::NotesClient::open(&path, Vec::new()).await {
                            Ok(client) => {
                                if let Err(error) = client.capture(&text).await {
                                    eprintln!("[remarked] capture failed: {error}");
                                }
                            }
                            Err(error) => eprintln!("[remarked] open notebook failed: {error}"),
                        }
                    });
                }
            }
            RemarkedInput::SetShowFinished(value) => {
                self.show_finished = value;
                self.reconcile(&sender);
            }
            RemarkedInput::StartTask { key, project } => {
                relm4::spawn(async move {
                    let action = client::TaskAction::Start { key, project };
                    if let Err(error) = client::task_action(action).await {
                        eprintln!("[remarked] start task failed: {error}");
                    }
                });
            }
            RemarkedInput::StopTracking => {
                if let Some(project) = self.catalog.project().cloned() {
                    relm4::spawn(async move {
                        if let Err(error) = client::task_action(client::TaskAction::Clear {
                            project: project.id,
                        })
                        .await
                        {
                            eprintln!("[remarked] stop tracking failed: {error}");
                        }
                    });
                }
            }
            RemarkedInput::SetTaskStatus { key, status } => {
                relm4::spawn(async move {
                    let action = client::TaskAction::Status { key, status };
                    if let Err(error) = client::task_action(action).await {
                        eprintln!("[remarked] status change failed: {error}");
                    }
                });
            }
            RemarkedInput::NoteFilter(value) => {
                self.note_filter = value;
                self.rebuild_note_grid(&sender);
                self.clear_changed();
            }
            RemarkedInput::TagsChanged => {
                self.note_dirty = self.selected_note.get().is_some()
                    && self.tags_entry.text() != self.original_tags;
                self.update_editor_state();
                self.clear_changed();
            }
            RemarkedInput::NoteChanged => {
                let text = self
                    .source_view
                    .buffer()
                    .text(
                        &self.source_view.buffer().start_iter(),
                        &self.source_view.buffer().end_iter(),
                        false,
                    )
                    .to_string();
                self.note_dirty = self.selected_note.get().is_some() && text != self.note_text;
                self.update_editor_state();
                self.clear_changed();
            }
            RemarkedInput::SaveNote => {
                self.save_note(&sender);
                self.update_editor_state();
                self.clear_changed();
            }
            RemarkedInput::ReloadNote => {}
            RemarkedInput::NoteLoaded(result) => {
                self.note_busy = false;
                match result {
                    Ok(body) => self.set_note_body(body),
                    Err(error) => eprintln!("[remarked] load note failed: {error}"),
                }
                self.update_editor_state();
                self.clear_changed();
            }
            RemarkedInput::NoteSaved(result) => {
                self.note_busy = false;
                match result {
                    Ok(_patch) => {
                        self.original_tags = self.tags_entry.text().to_string();
                        self.note_text = self
                            .source_view
                            .buffer()
                            .text(
                                &self.source_view.buffer().start_iter(),
                                &self.source_view.buffer().end_iter(),
                                false,
                            )
                            .to_string();
                        markdown::render(&self.note_preview, &self.note_text);
                        self.note_dirty = false;
                        if let Some(id) = self.selected_note.get() {
                            self.load_note(&sender, id);
                        }
                    }
                    Err(error) => eprintln!("[remarked] save note failed: {error}"),
                }
                self.update_editor_state();
                self.clear_changed();
            }
        }
    }
}

fn list_box_row_identity(row: &gtk::ListBoxRow) -> String {
    if !row.widget_name().is_empty() {
        return row.widget_name().to_string();
    }
    row.child()
        .map(|child| child.widget_name().to_string())
        .unwrap_or_default()
}

fn task_row_inits(
    nodes: Vec<model::TaskNode>,
    collapsed_tasks: &std::rc::Rc<std::cell::RefCell<std::collections::HashSet<String>>>,
    tag_counts: &StateSignal<source::TagCounts>,
) -> Vec<TaskRowInit> {
    let collapsed = collapsed_tasks.borrow();
    let mut out = Vec::new();
    fn walk(
        node: model::TaskNode,
        collapsed: &std::collections::HashSet<String>,
        tag_counts: &StateSignal<source::TagCounts>,
        out: &mut Vec<TaskRowInit>,
    ) {
        let is_collapsed = collapsed.contains(&node.key);
        out.push(TaskRowInit {
            instance_id: node.instance_id.clone(),
            key: node.key.clone(),
            project_id: node.project_id.clone(),
            depth: node.depth,
            has_children: node.has_children,
            collapsed: is_collapsed,
            tag_counts: tag_counts.clone(),
        });
        if !is_collapsed {
            for child in node.children {
                walk(child, collapsed, tag_counts, out);
            }
        }
    }
    for node in nodes {
        walk(node, &collapsed, &tag_counts, &mut out);
    }
    out
}

fn apply_task_row_output(
    model: &mut RemarkedWindow,
    sender: &ComponentSender<RemarkedWindow>,
    output: TaskRowOutput,
) {
    match output {
        TaskRowOutput::ToggleCollapse(key) => {
            let mut collapsed = model.collapsed_tasks.borrow_mut();
            if !collapsed.remove(&key) {
                collapsed.insert(key);
            }
            drop(collapsed);
            model.rebuild_task_list(sender);
        }
        TaskRowOutput::SetStatus { key, status } => {
            relm4::spawn(async move {
                if let Err(error) =
                    client::task_action(client::TaskAction::Status { key, status }).await
                {
                    eprintln!("[remarked] status change failed: {error}");
                }
            });
        }
        TaskRowOutput::Start { key, project } => {
            relm4::spawn(async move {
                if let Err(error) =
                    client::task_action(client::TaskAction::Start { key, project }).await
                {
                    eprintln!("[remarked] start task failed: {error}");
                }
            });
        }
        TaskRowOutput::Stop => {
            if let Some(project) = model.catalog.project().cloned() {
                relm4::spawn(async move {
                    if let Err(error) = client::task_action(client::TaskAction::Clear {
                        project: project.id,
                    })
                    .await
                    {
                        eprintln!("[remarked] stop tracking failed: {error}");
                    }
                });
            }
        }
    }
}

fn build_task_pane(
    sender: &ComponentSender<RemarkedWindow>,
) -> (gtk::Box, gtk::Stack, gtk::ListBox, gtk::Switch, gtk::Label) {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .vexpand(true)
        .hexpand(true)
        .build();
    content.add_css_class("remarked-pane");
    content.add_css_class("remarked-task-pane");

    let header = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .css_classes(["remarked-pane-header"])
        .build();
    let title_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .hexpand(true)
        .build();
    let title = gtk::Label::builder()
        .label("Tasks")
        .halign(gtk::Align::Start)
        .css_classes(["heading"])
        .build();
    let header_subtitle = gtk::Label::builder()
        .label("No project")
        .halign(gtk::Align::Start)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .css_classes(["dim-label", "caption"])
        .build();
    title_box.append(&title);
    title_box.append(&header_subtitle);
    header.append(&title_box);
    content.append(&header);

    let task_list = gtk::ListBox::new();
    task_list.set_selection_mode(gtk::SelectionMode::Single);
    task_list.set_activate_on_single_click(true);
    task_list.add_css_class("navigation-sidebar");
    task_list.set_vexpand(true);
    task_list.set_hexpand(true);

    let list_scrolled = gtk::ScrolledWindow::builder()
        .child(&task_list)
        .vexpand(true)
        .hexpand(true)
        .build();
    list_scrolled.add_css_class("remarked-task-list-scroll");
    let tx = sender.input_sender().clone();
    task_list.connect_row_selected(move |_, row| {
        let Some(row) = row else {
            return;
        };
        let id = list_box_row_identity(row);
        if !id.is_empty() {
            let _ = tx.send(RemarkedInput::SelectTask(id));
        }
    });

    let empty = adw::StatusPage::builder()
        .title("No tasks yet")
        .description("Tasks for the focused project will appear here.")
        .icon_name("checkbox-symbolic")
        .vexpand(true)
        .hexpand(true)
        .build();
    empty.add_css_class("remarked-empty");

    let task_stack = gtk::Stack::new();
    task_stack.set_vexpand(true);
    task_stack.set_hexpand(true);
    task_stack.add_named(&list_scrolled, Some("list"));
    task_stack.add_named(&empty, Some("empty"));
    task_stack.set_visible_child_name("list");
    content.append(&task_stack);

    let finished_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let finished_label = gtk::Label::new(Some("Show finished tasks"));
    finished_label.set_xalign(0.0);
    finished_label.set_hexpand(true);
    finished_row.append(&finished_label);
    finished_row.add_css_class("remarked-finished-row");
    let show_finished_switch = gtk::Switch::builder().valign(gtk::Align::Center).build();
    finished_row.append(&show_finished_switch);
    let tx = sender.input_sender().clone();
    show_finished_switch.connect_state_set(move |_, state| {
        let _ = tx.send(RemarkedInput::SetShowFinished(state));
        glib::Propagation::Proceed
    });
    content.append(&finished_row);

    (
        content,
        task_stack,
        task_list,
        show_finished_switch,
        header_subtitle,
    )
}

fn build_notes_surface(
    sender: &ComponentSender<RemarkedWindow>,
) -> (
    gtk::Box,
    gtk::FlowBox,
    gtk::Stack,
    gtk::Entry,
    gtk::Button,
    gtk::ToggleButton,
) {
    let surface = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(14)
        .vexpand(true)
        .hexpand(true)
        .build();
    surface.add_css_class("remarked-pane");
    surface.add_css_class("remarked-content");
    surface.add_css_class("remarked-notes-surface");

    let header = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .css_classes(["remarked-pane-header"])
        .build();
    let title = gtk::Label::builder()
        .label("Notes")
        .halign(gtk::Align::Start)
        .hexpand(true)
        .css_classes(["heading"])
        .build();
    let tasks_toggle = gtk::ToggleButton::builder()
        .icon_name("view-conceal-symbolic")
        .tooltip_text("Hide tasks")
        .active(true)
        .css_classes(["flat", "circular"])
        .build();
    let tx = sender.input_sender().clone();
    tasks_toggle.connect_toggled(move |button| {
        let _ = tx.send(RemarkedInput::ToggleTaskPane(Some(button.is_active())));
    });
    let clear_button = gtk::Button::builder()
        .label("Clear filter")
        .css_classes(["flat"])
        .visible(false)
        .build();
    let refresh_button = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Refresh")
        .css_classes(["flat", "circular"])
        .build();
    header.append(&title);
    header.append(&tasks_toggle);
    header.append(&clear_button);
    header.append(&refresh_button);
    surface.append(&header);

    let note_filter_entry = gtk::Entry::builder()
        .placeholder_text("Filter notes by title or tag")
        .primary_icon_name("edit-find-symbolic")
        .hexpand(true)
        .build();
    let tx = sender.input_sender().clone();
    note_filter_entry.connect_changed(move |entry| {
        let _ = tx.send(RemarkedInput::NoteFilter(entry.text().to_string()));
    });
    surface.append(&note_filter_entry);

    let note_grid = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .activate_on_single_click(true)
        .column_spacing(12)
        .row_spacing(12)
        .homogeneous(true)
        .max_children_per_line(8)
        .valign(gtk::Align::Start)
        .vexpand(true)
        .build();
    note_grid.add_css_class("remarked-notes-grid");
    let tx = sender.input_sender().clone();
    note_grid.connect_child_activated(move |_, child| {
        let id = child.widget_name();
        if !id.is_empty() {
            let _ = tx.send(RemarkedInput::SelectNote(id.to_string()));
        }
    });

    let grid_scrolled = gtk::ScrolledWindow::builder()
        .child(&note_grid)
        .vexpand(true)
        .hexpand(true)
        .build();
    grid_scrolled.add_css_class("remarked-grid-scroll");

    let empty = adw::StatusPage::builder()
        .title("No notes yet")
        .description("Select a task to seed its main note, or press Ctrl+N.")
        .icon_name("document-new-symbolic")
        .vexpand(true)
        .hexpand(true)
        .build();
    empty.add_css_class("remarked-empty");

    let notes_stack = gtk::Stack::new();
    notes_stack.set_vexpand(true);
    notes_stack.set_hexpand(true);
    notes_stack.add_named(&grid_scrolled, Some("grid"));
    notes_stack.add_named(&empty, Some("empty"));
    notes_stack.set_visible_child_name("grid");
    surface.append(&notes_stack);

    let tx = sender.input_sender().clone();
    clear_button.connect_clicked(move |_| {
        let _ = tx.send(RemarkedInput::ClearTask);
    });

    (
        surface,
        note_grid,
        notes_stack,
        note_filter_entry,
        clear_button,
        tasks_toggle,
    )
}

fn build_note_sheet(
    sender: &ComponentSender<RemarkedWindow>,
) -> (
    gtk::Box,
    gtk::Label,
    gtk::Box,
    sourceview5::View,
    gtk::Box,
    gtk::Stack,
    gtk::ToggleButton,
    gtk::Entry,
    gtk::Button,
) {
    let sheet_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .vexpand(true)
        .hexpand(true)
        .build();
    sheet_box.add_css_class("remarked-sheet-content");

    let header = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .css_classes(["remarked-sheet-header"])
        .build();
    let title_label = gtk::Label::builder()
        .label("Note")
        .halign(gtk::Align::Start)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .css_classes(["heading"])
        .build();
    let edit_toggle = gtk::ToggleButton::builder()
        .icon_name("document-edit-symbolic")
        .tooltip_text("Edit Markdown")
        .css_classes(["flat", "circular"])
        .build();
    let tx = sender.input_sender().clone();
    edit_toggle.connect_toggled(move |button| {
        let _ = tx.send(RemarkedInput::SetEditingNote(button.is_active()));
    });
    let save_button = gtk::Button::builder()
        .label("Save")
        .css_classes(["suggested-action"])
        .sensitive(false)
        .visible(false)
        .build();
    let tx = sender.input_sender().clone();
    save_button.connect_clicked(move |_| {
        let _ = tx.send(RemarkedInput::SaveNote);
    });
    let close_button = gtk::Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text("Close note")
        .css_classes(["flat", "circular"])
        .build();
    let tx = sender.input_sender().clone();
    close_button.connect_clicked(move |_| {
        let _ = tx.send(RemarkedInput::ClearNote);
    });
    header.append(&title_label);
    header.append(&edit_toggle);
    header.append(&save_button);
    header.append(&close_button);
    sheet_box.append(&header);

    let tags_entry = gtk::Entry::builder()
        .placeholder_text("Tags, separated by commas")
        .editable(false)
        .visible(false)
        .hexpand(true)
        .build();
    let tx = sender.input_sender().clone();
    tags_entry.connect_changed(move |_| {
        let _ = tx.send(RemarkedInput::TagsChanged);
    });
    sheet_box.append(&tags_entry);

    let buffer = sourceview5::Buffer::new(None);
    buffer.set_language(
        sourceview5::LanguageManager::default()
            .language("markdown")
            .as_ref(),
    );
    buffer.set_highlight_syntax(true);
    buffer.set_enable_undo(true);
    set_source_scheme(&buffer);
    let scheme_buffer = buffer.downgrade();
    adw::StyleManager::default().connect_dark_notify(move |_| {
        if let Some(buffer) = scheme_buffer.upgrade() {
            set_source_scheme(&buffer);
        }
    });

    let source_view = sourceview5::View::with_buffer(&buffer);
    source_view.set_vexpand(true);
    source_view.set_hexpand(true);
    source_view.set_wrap_mode(gtk::WrapMode::WordChar);
    source_view.set_monospace(true);
    source_view.add_css_class("remarked-editor");
    let tx = sender.input_sender().clone();
    buffer.connect_changed(move |_| {
        let _ = tx.send(RemarkedInput::NoteChanged);
    });

    let scrolled = gtk::ScrolledWindow::builder()
        .child(&source_view)
        .vexpand(true)
        .hexpand(true)
        .build();

    let preview = gtk::Box::new(gtk::Orientation::Vertical, 12);
    preview.add_css_class("remarked-preview");
    let preview_scrolled = gtk::ScrolledWindow::builder()
        .child(&preview)
        .vexpand(true)
        .hexpand(true)
        .build();
    preview_scrolled.add_css_class("remarked-preview-scroll");

    let editor_stack = gtk::Stack::new();
    editor_stack.set_vexpand(true);
    editor_stack.set_hexpand(true);
    editor_stack.add_named(&preview_scrolled, Some("preview"));
    editor_stack.add_named(&scrolled, Some("edit"));
    editor_stack.set_visible_child_name("preview");

    let editor_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    editor_box.set_vexpand(true);
    editor_box.set_hexpand(true);
    editor_box.set_height_request(560);
    editor_box.append(&editor_stack);
    sheet_box.append(&editor_box);

    (
        sheet_box,
        title_label,
        editor_box,
        source_view,
        preview,
        editor_stack,
        edit_toggle,
        tags_entry,
        save_button,
    )
}

fn set_source_scheme(buffer: &sourceview5::Buffer) {
    let name = if adw::StyleManager::default().is_dark() {
        "Adwaita-dark"
    } else {
        "Adwaita"
    };
    buffer.set_style_scheme(
        sourceview5::StyleSchemeManager::default()
            .scheme(name)
            .as_ref(),
    );
}

fn load_styles(window: &adw::ApplicationWindow) {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(
        "
        window.remarked-window,
        .remarked-root {
            background: transparent;
        }
        ",
    );
    gtk::style_context_add_provider_for_display(
        &WidgetExt::display(window),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn add_keyboard_handlers(
    window: &adw::ApplicationWindow,
    sender: &ComponentSender<RemarkedWindow>,
) {
    let controller = gtk::EventControllerKey::new();
    let s = sender.clone();
    controller.connect_key_pressed(move |_, keyval, _keycode, mods| {
        if keyval == gtk::gdk::Key::Escape {
            s.input(RemarkedInput::Hide);
            glib::Propagation::Stop
        } else if keyval == gtk::gdk::Key::n && mods.contains(gdk::ModifierType::CONTROL_MASK) {
            s.input(RemarkedInput::CaptureNote(String::new()));
            glib::Propagation::Stop
        } else if keyval == gtk::gdk::Key::e && mods.contains(gdk::ModifierType::CONTROL_MASK) {
            s.input(RemarkedInput::SetEditingNote(true));
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(controller);
}
