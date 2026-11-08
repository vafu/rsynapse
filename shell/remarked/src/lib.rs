//! Rsynapse's shell-like task-and-notes overlay (v2).
use adw::prelude::*;
use gtk::{gdk, glib, prelude::WidgetExt};
use relm4::prelude::*;
use shell_core::{
    gtk4_layer_shell::{KeyboardMode, LayerShell},
    source::{StateSignal, workspace_project},
    window::{Anchors, Layer, WindowConfig},
};
use sourceview5::prelude::*;

mod client;
mod model;
mod source;

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
    ToggleTaskCollapse(String),
    NoteFilter(String),
    TagsChanged,
    NoteChanged,
    SaveNote,
    ReloadNote,
    NoteLoaded(Result<notebook_api::note::NoteBody, String>),
    NoteSaved(Result<notebook_api::note::NoteMutationResult, String>),
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
    task_list: gtk::ListBox,
    note_pane: gtk::Box,
    note_list: gtk::ListBox,
    note_filter_entry: gtk::Entry,
    editor_box: gtk::Box,
    source_view: sourceview5::View,
    tags_entry: gtk::Entry,
    save_button: gtk::Button,
    editor_status_page: adw::StatusPage,
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
    visible: bool,
    collapsed_tasks: std::collections::HashSet<String>,
    main_note_attempted: std::collections::HashSet<String>,
    task_rows_by_id: std::collections::HashMap<String, adw::ActionRow>,
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
        }
        self.selected_task.set(task);
    }

    fn select_note(&mut self, note: Option<String>) {
        self.selected_note.set(note);
        self.note_text.clear();
        self.original_tags.clear();
        self.note_dirty = false;
        self.note_busy = false;
    }

    fn set_note_body(&mut self, body: notebook_api::note::NoteBody) {
        let buffer = self.source_view.buffer();
        buffer.set_text(&body.text);
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

        self.rebuild_note_list(sender);
        self.ensure_main_note();
        self.auto_select_note(sender);
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

    fn auto_select_note(&mut self, sender: &ComponentSender<Self>) {
        let Some(task_id) = self.selected_task.get() else {
            if self.selected_note.get().is_some() {
                self.select_note(None);
            }
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
        if self
            .selected_note
            .get()
            .as_ref()
            .is_some_and(|id| notes.iter().any(|note| note.id.to_string() == *id))
        {
            return;
        }
        let next = notes
            .iter()
            .find(|note| model::is_main_note(note, &task_id))
            .or_else(|| notes.first())
            .map(|note| note.id.to_string());
        match next {
            Some(id) => {
                self.select_note(Some(id.clone()));
                self.load_note(sender, id);
            }
            None => {
                self.select_note(None);
                self.source_view.buffer().set_text("");
                self.tags_entry.set_text("");
            }
        }
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
        self.refresh_panes(sender);
    }

    fn reconcile_notes(&mut self, sender: &ComponentSender<Self>) {
        self.refresh_panes(sender);
    }

    fn task_count_label(&self, instance_id: &str) -> String {
        match self.tag_counts.count(&model::task_tag(instance_id)) {
            Some(1) => "1 note".to_string(),
            Some(count) => format!("{count} notes"),
            None => String::new(),
        }
    }

    fn update_task_counts(&self) {
        for (instance_id, row) in &self.task_rows_by_id {
            row.set_subtitle(&self.task_count_label(instance_id));
        }
    }

    fn rebuild_task_list(&mut self, sender: &ComponentSender<Self>) {
        self.task_list.remove_all();
        self.task_rows_by_id.clear();
        let tasks: &[proj_model::GoalIdentityInfo] = match &self.catalog {
            source::Catalog::Ready { tasks, .. } => tasks,
            _ => &[],
        };
        let active: &[String] = match &self.catalog {
            source::Catalog::Ready { active, .. } => active,
            _ => &[],
        };
        for (task, depth, has_children) in
            model::task_rows(tasks, self.show_finished, &self.collapsed_tasks)
        {
            let escaped_title = glib::markup_escape_text(model::task_title(&task.goal.title));
            let title = if model::is_finished(&task.goal.status) {
                format!("<span alpha=\"55%\">{escaped_title}</span>")
            } else {
                escaped_title.to_string()
            };
            let row = adw::ActionRow::builder()
                .title(title)
                .use_markup(true)
                .title_lines(2)
                .subtitle(self.task_count_label(&task.instance_id))
                .subtitle_lines(1)
                .activatable(true)
                .build();
            row.add_css_class("remarked-task-row");

            let menu = build_task_menu(task, active.contains(&task.instance_id), sender);
            let click = gtk::GestureClick::builder()
                .button(gtk::gdk::BUTTON_SECONDARY)
                .build();
            let menu_for_click = menu.clone();
            click.connect_released(move |_, _, _, _| menu_for_click.popup());
            row.add_controller(click);
            row.add_prefix(&menu);
            if depth > 0 {
                let spacer = gtk::Box::builder()
                    .width_request(depth as i32 * 16)
                    .valign(gtk::Align::Center)
                    .build();
                row.add_prefix(&spacer);
            }

            if has_children {
                let icon = if self.collapsed_tasks.contains(&task.key) {
                    "pan-end-symbolic"
                } else {
                    "pan-down-symbolic"
                };
                let button = gtk::Button::builder()
                    .icon_name(icon)
                    .css_classes(["flat", "circular"])
                    .width_request(24)
                    .height_request(24)
                    .build();
                let key = task.key.clone();
                let tx = sender.input_sender().clone();
                button.connect_clicked(move |_| {
                    let _ = tx.send(RemarkedInput::ToggleTaskCollapse(key.clone()));
                });
                row.add_suffix(&button);
            }

            self.task_rows_by_id
                .insert(task.instance_id.clone(), row.clone());
            row.set_widget_name(&task.instance_id);
            self.task_list.append(&row);
        }
        if let Some(selected) = self.selected_task.get() {
            let mut child = self.task_list.first_child();
            while let Some(widget) = child {
                if let Some(row) = widget.downcast_ref::<gtk::ListBoxRow>() {
                    if row.widget_name().as_str() == selected.as_str() {
                        self.task_list.select_row(Some(row));
                        break;
                    }
                }
                child = widget.next_sibling();
            }
        }
        if self.task_list.observe_children().n_items() == 0 {
            let label = gtk::Label::builder()
                .label("No tasks for this project.")
                .halign(gtk::Align::Center)
                .valign(gtk::Align::Center)
                .vexpand(true)
                .css_classes(["dim-label"])
                .margin_top(24)
                .margin_bottom(24)
                .build();
            let row = gtk::ListBoxRow::builder()
                .child(&label)
                .activatable(false)
                .selectable(false)
                .build();
            self.task_list.append(&row);
        }
    }

    fn rebuild_note_list(&self, sender: &ComponentSender<Self>) {
        self.note_list.remove_all();
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
            let row = adw::ActionRow::builder()
                .title(&note.title)
                .subtitle(&note.tags.join(", "))
                .activatable(true)
                .build();
            row.add_css_class("remarked-row");
            if selected_task
                .as_ref()
                .is_some_and(|id| note.tags.contains(&model::task_tag(id)))
            {
                row.add_css_class("remarked-note-active");
            }
            let id = note.id.to_string();
            let tx = sender.input_sender().clone();
            row.connect_activated(move |_| {
                let _ = tx.send(RemarkedInput::SelectNote(id.clone()));
            });
            self.note_list.append(&row);
        }
        if self.note_list.observe_children().n_items() == 0 {
            let label = gtk::Label::builder()
                .label("No notes yet.")
                .halign(gtk::Align::Center)
                .valign(gtk::Align::Center)
                .vexpand(true)
                .css_classes(["dim-label"])
                .margin_top(24)
                .margin_bottom(24)
                .build();
            let row = gtk::ListBoxRow::builder()
                .child(&label)
                .activatable(false)
                .selectable(false)
                .build();
            self.note_list.append(&row);
        }
    }

    fn update_editor_state(&self) {
        if self.selected_note.get().is_some() {
            self.editor_box.set_visible(true);
            self.editor_status_page.set_visible(false);
            self.save_button
                .set_sensitive(self.note_dirty && !self.note_busy);
        } else {
            self.editor_box.set_visible(false);
            self.editor_status_page.set_visible(true);
        }
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
        let (mut width, mut height) = (i32::MAX, i32::MAX);
        for i in 0..monitors.n_items() {
            if let Some(monitor) = monitors.item(i).and_downcast::<gdk::Monitor>() {
                let geo = monitor.geometry();
                width = width.min(geo.width());
                height = height.min(geo.height());
            }
        }
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
            WindowConfig::new(Layer::Overlay)
                .with_anchors(Anchors::ALL)
                .with_namespace("rsynapse-remarked"),
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

        let toolbar = adw::ToolbarView::builder()
            .vexpand(true)
            .hexpand(true)
            .build();
        let header = adw::HeaderBar::builder()
            .show_end_title_buttons(true)
            .show_start_title_buttons(false)
            .build();
        let title_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        let title_label = gtk::Label::builder()
            .label("Tasks & Notes")
            .css_classes(["heading"])
            .build();
        let header_subtitle = gtk::Label::builder()
            .label("")
            .css_classes(["dim-label", "caption"])
            .build();
        title_box.append(&title_label);
        title_box.append(&header_subtitle);
        header.set_title_widget(Some(&title_box));
        toolbar.add_top_bar(&header);

        let (task_pane, task_list, show_finished_switch) = build_task_pane(&sender);
        task_pane.set_hexpand(false);
        task_pane.set_width_request(400);
        let tx = sender.input_sender().clone();
        task_list.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                let name = row.widget_name();
                if !name.is_empty() {
                    let _ = tx.send(RemarkedInput::SelectTask(name.to_string()));
                }
            }
        });

        let (
            note_pane,
            note_list,
            note_filter_entry,
            editor_box,
            source_view,
            tags_entry,
            save_button,
            editor_status_page,
            clear_filter_button,
        ) = build_note_pane(&sender);
        note_pane.set_hexpand(true);

        let main_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .vexpand(true)
            .hexpand(true)
            .build();
        main_box.append(&task_pane);
        main_box.append(&note_pane);

        toolbar.set_content(Some(&main_box));

        let root_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .vexpand(true)
            .hexpand(true)
            .build();
        root_box.add_css_class("remarked-root");
        root_box.append(&toolbar);
        root.set_content(Some(&root_box));

        add_keyboard_handlers(&root, &sender);

        let model = Self::new(
            root.clone(),
            main_box,
            task_pane,
            task_list,
            note_pane,
            note_list,
            note_filter_entry,
            editor_box,
            source_view,
            tags_entry,
            save_button,
            editor_status_page,
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
            std::collections::HashSet::new(),
            std::collections::HashSet::new(),
            std::collections::HashMap::new(),
        );
        let widgets = view_output!();
        model.reconcile(&sender);
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            RemarkedInput::Source(message) => {
                Self::update(self, message);
                if self.changed(sources::Field::Catalog) {
                    self.reconcile_catalog(&sender);
                } else if self.changed(sources::Field::Notes) {
                    self.reconcile_notes(&sender);
                } else if self.changed(sources::Field::TagCounts) {
                    self.update_task_counts();
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
            RemarkedInput::ToggleTaskCollapse(key) => {
                if self.collapsed_tasks.contains(&key) {
                    self.collapsed_tasks.remove(&key);
                } else {
                    self.collapsed_tasks.insert(key);
                }
                self.rebuild_task_list(&sender);
                self.clear_changed();
            }
            RemarkedInput::SelectNote(id) => {
                self.select_note(Some(id.clone()));
                self.load_note(&sender, id);
                self.update_editor_state();
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
                self.rebuild_note_list(&sender);
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

const TASK_STATUSES: [&str; 5] = [
    "planned",
    "in-progress",
    "completed",
    "deferred",
    "cancelled",
];

fn status_icon(status: &str, active: bool) -> &'static str {
    if active {
        "find-location-symbolic"
    } else {
        match status {
            "completed" => "emblem-ok-symbolic",
            "cancelled" => "process-stop-symbolic",
            "in-progress" => "media-playback-start-symbolic",
            "deferred" => "media-playback-pause-symbolic",
            _ => "radio-symbolic",
        }
    }
}

fn build_task_menu(
    task: &proj_model::GoalIdentityInfo,
    active: bool,
    sender: &ComponentSender<RemarkedWindow>,
) -> gtk::MenuButton {
    use gtk::glib::variant::ToVariant;

    let actions = gtk::gio::SimpleActionGroup::new();

    let status_action = gtk::gio::SimpleAction::new_stateful(
        "status",
        Some(glib::VariantTy::STRING),
        &task.goal.status.to_variant(),
    );
    let key = task.key.clone();
    let tx = sender.input_sender().clone();
    status_action.connect_activate(move |action, parameter| {
        let Some(parameter) = parameter else {
            return;
        };
        let Some(status) = parameter.str() else {
            return;
        };
        action.set_state(parameter);
        let _ = tx.send(RemarkedInput::SetTaskStatus {
            key: key.clone(),
            status: status.to_string(),
        });
    });
    actions.add_action(&status_action);

    let start_action = gtk::gio::SimpleAction::new("start", None);
    let key = task.key.clone();
    let project = task.goal.project_id.clone();
    let tx = sender.input_sender().clone();
    start_action.connect_activate(move |_, _| {
        let _ = tx.send(RemarkedInput::StartTask {
            key: key.clone(),
            project: project.clone(),
        });
    });
    actions.add_action(&start_action);

    let stop_action = gtk::gio::SimpleAction::new("stop", None);
    let tx = sender.input_sender().clone();
    stop_action.connect_activate(move |_, _| {
        let _ = tx.send(RemarkedInput::StopTracking);
    });
    actions.add_action(&stop_action);

    let status_menu = gtk::gio::Menu::new();
    for status in TASK_STATUSES {
        let item = gtk::gio::MenuItem::new(Some(&status.replace('-', " ")), None);
        item.set_action_and_target_value(Some("task.status"), Some(&status.to_variant()));
        status_menu.append_item(&item);
    }

    let menu = gtk::gio::Menu::new();
    menu.append_section(None, &status_menu);

    let tracking = gtk::gio::Menu::new();
    let mut has_tracking = true;
    if active {
        tracking.append(Some("Stop tracking"), Some("task.stop"));
    } else if !model::is_finished(&task.goal.status) {
        tracking.append(Some("Start tracking"), Some("task.start"));
    } else {
        has_tracking = false;
    }
    if has_tracking {
        menu.append_section(None, &tracking);
    }

    let button = gtk::MenuButton::builder()
        .icon_name(status_icon(&task.goal.status, active))
        .tooltip_text("Task actions")
        .css_classes(["flat", "circular"])
        .build();
    button.set_menu_model(Some(&menu));
    button.insert_action_group("task", Some(&actions));
    button
}

fn build_task_pane(
    sender: &ComponentSender<RemarkedWindow>,
) -> (gtk::Box, gtk::ListBox, gtk::Switch) {
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
        .css_classes(["toolbar"])
        .build();
    let title = gtk::Label::builder()
        .label("Tasks")
        .halign(gtk::Align::Start)
        .hexpand(true)
        .css_classes(["heading"])
        .build();
    header.append(&title);
    content.append(&header);

    let list_box = gtk::ListBox::builder()
        .css_classes(["boxed-list"])
        .selection_mode(gtk::SelectionMode::Single)
        .vexpand(true)
        .build();
    content.append(&list_box);

    let finished_row = adw::ActionRow::builder()
        .title("Show finished tasks")
        .build();
    let show_finished_switch = gtk::Switch::builder().valign(gtk::Align::Center).build();
    finished_row.add_suffix(&show_finished_switch);
    finished_row.set_activatable_widget(Some(&show_finished_switch));
    let tx = sender.input_sender().clone();
    show_finished_switch.connect_state_set(move |_, state| {
        let _ = tx.send(RemarkedInput::SetShowFinished(state));
        glib::Propagation::Proceed
    });
    content.append(&finished_row);

    (content, list_box, show_finished_switch)
}

fn build_note_pane(
    sender: &ComponentSender<RemarkedWindow>,
) -> (
    gtk::Box,
    gtk::ListBox,
    gtk::Entry,
    gtk::Box,
    sourceview5::View,
    gtk::Entry,
    gtk::Button,
    adw::StatusPage,
    gtk::Button,
) {
    let split = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .vexpand(true)
        .hexpand(true)
        .build();

    let sidebar = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .width_request(260)
        .vexpand(true)
        .hexpand(false)
        .build();
    sidebar.add_css_class("remarked-pane");

    let header = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .css_classes(["toolbar"])
        .build();
    let title = gtk::Label::builder()
        .label("Notes")
        .halign(gtk::Align::Start)
        .hexpand(true)
        .css_classes(["heading"])
        .build();
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
    header.append(&clear_button);
    header.append(&refresh_button);
    sidebar.append(&header);

    let note_filter_entry = gtk::Entry::builder()
        .placeholder_text("Filter notes by title or tag")
        .primary_icon_name("edit-find-symbolic")
        .hexpand(true)
        .build();
    let tx = sender.input_sender().clone();
    note_filter_entry.connect_changed(move |entry| {
        let _ = tx.send(RemarkedInput::NoteFilter(entry.text().to_string()));
    });
    sidebar.append(&note_filter_entry);

    let list_box = gtk::ListBox::builder()
        .css_classes(["boxed-list"])
        .selection_mode(gtk::SelectionMode::Single)
        .vexpand(true)
        .build();
    sidebar.append(&list_box);

    split.append(&sidebar);

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .vexpand(true)
        .hexpand(true)
        .build();
    content.add_css_class("remarked-pane");

    let header = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .css_classes(["toolbar"])
        .build();
    let header_title = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .hexpand(true)
        .build();
    let title_label = gtk::Label::builder()
        .label("Select a note")
        .css_classes(["heading"])
        .build();
    let sub_label = gtk::Label::builder()
        .label("Markdown")
        .css_classes(["dim-label", "caption"])
        .build();
    header_title.append(&title_label);
    header_title.append(&sub_label);
    header.append(&header_title);
    let save_button = gtk::Button::builder()
        .label("Save")
        .css_classes(["suggested-action"])
        .sensitive(false)
        .build();
    let tx = sender.input_sender().clone();
    save_button.connect_clicked(move |_| {
        let _ = tx.send(RemarkedInput::SaveNote);
    });
    header.append(&save_button);
    content.append(&header);

    let tags_entry = gtk::Entry::builder()
        .placeholder_text("Tags, separated by commas")
        .hexpand(true)
        .build();
    let tx = sender.input_sender().clone();
    tags_entry.connect_changed(move |_| {
        let _ = tx.send(RemarkedInput::TagsChanged);
    });
    content.append(&tags_entry);

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
    source_view.add_css_class("frame");
    let tx = sender.input_sender().clone();
    buffer.connect_changed(move |_| {
        let _ = tx.send(RemarkedInput::NoteChanged);
    });

    let scrolled = gtk::ScrolledWindow::builder()
        .child(&source_view)
        .vexpand(true)
        .hexpand(true)
        .build();

    let editor_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .vexpand(true)
        .hexpand(true)
        .build();
    editor_box.append(&scrolled);
    content.append(&editor_box);

    let status_page = adw::StatusPage::builder()
        .title("Your project knowledge")
        .description("Select a note, or create one with Ctrl+N.")
        .icon_name("document-edit-symbolic")
        .vexpand(true)
        .hexpand(true)
        .visible(false)
        .build();
    content.append(&status_page);

    split.append(&content);

    let tx = sender.input_sender().clone();
    clear_button.connect_clicked(move |_| {
        let _ = tx.send(RemarkedInput::ClearTask);
    });

    (
        split,
        list_box,
        note_filter_entry,
        editor_box,
        source_view,
        tags_entry,
        save_button,
        status_page,
        clear_button,
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
        .remarked-pane {
            padding: 12px;
        }
        .remarked-task-details {
            padding: 16px;
        }
        .remarked-row {
            margin: 4px 0;
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
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(controller);
}
