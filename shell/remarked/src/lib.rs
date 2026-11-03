//! Rsynapse's shell-like task-and-notes overlay (v2).
use adw::prelude::*;
use gtk::{gdk, glib, prelude::WidgetExt};
use relm4::prelude::*;
use shell_core::{
    gtk4_layer_shell::{KeyboardMode, LayerShell},
    source::StateSignal,
    window::{Anchors, Layer, WindowConfig},
};

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
    SetProject(Option<String>),
    SelectTask(String),
    ClearTask,
    SelectNote(String),
    ClearNote,
    CaptureNote(String),
    SetShowFinished(bool),
    StartTask,
    StopTracking,
    SetTaskStatus(String),
    ToggleTaskDetails,
    ToggleTaskCollapse(String),
    NoteFilter(String),
    TagsChanged,
    NoteChanged,
    SaveNote,
    ReloadNote,
    NoteLoaded(Result<notebook_api::note::NoteBody, String>),
    NoteSaved(Result<notebook_api::note::PatchResult, String>),
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
    task_split: adw::OverlaySplitView,
    task_list: gtk::ListBox,
    task_details_button: gtk::Button,
    task_title: gtk::Label,
    task_description: gtk::Label,
    task_start_button: gtk::Button,
    task_status_drop: gtk::DropDown,
    note_split: adw::OverlaySplitView,
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
    project_id: StateSignal<Option<String>>,
    show_finished: bool,
    note_filter: String,
    note_text: String,
    original_tags: String,
    note_dirty: bool,
    note_busy: bool,
    visible: bool,
    collapsed_tasks: std::collections::HashSet<String>,
    #[source(source::catalog(project_id.observable()))]
    catalog: source::Catalog,
    #[source(source::notes(notebook.observable(), selected_task.observable()))]
    notes: source::Notes,
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
        let tags = body.tags.join(", ");
        self.tags_entry.set_text(&tags);
        self.original_tags = tags;
        self.note_text = body.text;
        self.note_dirty = false;
        self.note_busy = false;
    }

    fn reconcile(&mut self, sender: &ComponentSender<Self>) {
        if let Some(project) = self.catalog.project() {
            self.header_subtitle.set_label(&project.name);
            let cwd = &project.cwd;
            if !cwd.is_empty() {
                self.notebook.set(Some(source::Notebook {
                    path: format!("{cwd}/.notes"),
                    generation: 0,
                }));
            }
        } else {
            self.header_subtitle.set_label("No project");
            self.notebook.set(None);
        }

        if let Some(id) = self.selected_task.get() {
            let tasks = self.catalog.tasks();
            if !tasks.iter().any(|task| task.instance_id == id) {
                self.selected_task.set(None);
            }
        }

        self.rebuild_task_list(sender);
        self.update_task_details();
        self.rebuild_note_list(sender);
        if let Some(id) = self.selected_note.get() {
            let notes = match &self.notes {
                source::Notes::Ready(_, notes) => notes.as_slice(),
                _ => &[],
            };
            if !notes.iter().any(|note| note.id.to_string() == id) {
                self.select_note(None);
                self.source_view.buffer().set_text("");
                self.tags_entry.set_text("");
            }
        }
        self.update_editor_state();
        self.clear_changed();
    }

    fn rebuild_task_list(&self, sender: &ComponentSender<Self>) {
        self.task_list.remove_all();
        let tasks = self.catalog.tasks();
        let active = self.catalog.active();
        for (task, depth, has_children) in
            model::task_rows(tasks, self.show_finished, &self.collapsed_tasks)
        {
            let row = gtk::ListBoxRow::builder().activatable(true).build();
            row.add_css_class("remarked-task-row");
            if depth > 0 {
                row.set_margin_start(depth as i32 * 16);
            }

            let hbox = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(8)
                .margin_top(6)
                .margin_bottom(6)
                .margin_start(8)
                .margin_end(8)
                .build();

            let expander = if has_children {
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
                button.upcast::<gtk::Widget>()
            } else {
                gtk::Box::builder()
                    .width_request(24)
                    .build()
                    .upcast::<gtk::Widget>()
            };
            hbox.append(&expander);

            let status_icon = if active.contains(&task.instance_id) {
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
            let icon = gtk::Image::builder()
                .icon_name(status_icon)
                .pixel_size(16)
                .valign(gtk::Align::Center)
                .build();
            hbox.append(&icon);

            let vbox = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(2)
                .hexpand(true)
                .build();
            let title = gtk::Label::builder()
                .label(&task.goal.title)
                .halign(gtk::Align::Start)
                .wrap(true)
                .wrap_mode(gtk::pango::WrapMode::Word)
                .build();
            let subtitle = gtk::Label::builder()
                .label(&task.goal.status.replace('-', " "))
                .halign(gtk::Align::Start)
                .css_classes(["dim-label", "caption"])
                .build();
            if model::is_finished(&task.goal.status) {
                title.add_css_class("dim-label");
            }
            vbox.append(&title);
            vbox.append(&subtitle);
            hbox.append(&vbox);

            row.set_child(Some(&hbox));
            row.set_widget_name(&task.instance_id);
            self.task_list.append(&row);
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

    fn update_task_details(&self) {
        if let Some(task) = self.selected_task_info() {
            self.task_split.set_show_sidebar(true);
            self.task_details_button
                .set_icon_name("sidebar-hide-symbolic");
            self.task_title.set_label(&task.goal.title);
            self.task_description.set_label(&task.goal.description);
            self.task_start_button
                .set_visible(!model::is_finished(&task.goal.status));

            let statuses = [
                "planned",
                "in-progress",
                "completed",
                "deferred",
                "cancelled",
            ];
            let position = statuses
                .iter()
                .position(|s| *s == task.goal.status)
                .unwrap_or(0) as u32;
            self.task_status_drop.set_selected(position);
        } else {
            self.task_split.set_show_sidebar(false);
            self.task_details_button
                .set_icon_name("document-properties-symbolic");
            self.task_title.set_label("");
            self.task_description.set_label("");
        }
    }

    fn toggle_task_details(&mut self) {
        let show = !self.task_split.shows_sidebar();
        self.task_split.set_show_sidebar(show);
        if !show {
            self.selected_task.set(None);
        }
    }

    fn rebuild_note_list(&self, sender: &ComponentSender<Self>) {
        self.note_list.remove_all();
        let notes = match &self.notes {
            source::Notes::Ready(_, notes) => notes,
            _ => return,
        };

        let filter = self.note_filter.trim().to_lowercase();
        let selected_task = self.selected_task.get();
        self.clear_filter_button
            .set_visible(selected_task.is_some());
        for note in notes {
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
            source::Notes::Ready(path, _) => Some(path.clone()),
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
        self.note_busy = true;
        let tx = sender.input_sender().clone();
        relm4::spawn(async move {
            let result = async {
                let client = client::NotesClient::open(&path, Vec::new()).await?;
                let body = client.body(&id.clone().into()).await?;
                client
                    .save(&id.into(), &body.revision.into(), &text, tags)
                    .await
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
    type Init = Option<String>;
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
        project_id: Self::Init,
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

        let (
            task_split,
            task_list,
            task_details_button,
            task_title,
            task_description,
            task_start_button,
            task_status_drop,
            show_finished_switch,
        ) = build_task_pane(&sender);
        task_split.set_hexpand(false);
        task_split.set_width_request(400);
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
            note_split,
            note_list,
            note_filter_entry,
            editor_box,
            source_view,
            tags_entry,
            save_button,
            editor_status_page,
            clear_filter_button,
        ) = build_note_pane(&sender);
        note_split.set_hexpand(true);

        let main_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .vexpand(true)
            .hexpand(true)
            .build();
        main_box.append(&task_split);
        main_box.append(&note_split);

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
            task_split,
            task_list,
            task_details_button,
            task_title,
            task_description,
            task_start_button,
            task_status_drop,
            note_split,
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
            StateSignal::new(project_id),
            false,
            String::new(),
            String::new(),
            String::new(),
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
            RemarkedInput::Source(message) => {
                Self::update(self, message);
                self.reconcile(&sender);
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
            RemarkedInput::SetProject(project_id) => {
                self.project_id.set(project_id);
                self.selected_task.set(None);
                self.select_note(None);
                self.source_view.buffer().set_text("");
                self.tags_entry.set_text("");
                self.note_filter = String::new();
                self.note_filter_entry.set_text("");
                self.reconcile(&sender);
            }
            RemarkedInput::SelectTask(id) => {
                self.select_task(Some(id));
                self.reconcile(&sender);
            }
            RemarkedInput::ClearTask => {
                self.select_task(None);
                self.reconcile(&sender);
            }
            RemarkedInput::ToggleTaskDetails => {
                self.toggle_task_details();
                self.clear_changed();
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
                self.reconcile(&sender);
            }
            RemarkedInput::ClearNote => {
                self.select_note(None);
                self.reconcile(&sender);
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
            RemarkedInput::StartTask => {
                if let Some(task) = self.selected_task_info().cloned() {
                    let tx = sender.input_sender().clone();
                    relm4::spawn(async move {
                        let action = client::TaskAction::Start {
                            key: task.key.clone(),
                            project: task.goal.project_id.clone(),
                        };
                        if let Err(error) = client::task_action(action).await {
                            eprintln!("[remarked] start task failed: {error}");
                        }
                        let _ = tx.send(RemarkedInput::ReloadNote);
                    });
                }
            }
            RemarkedInput::StopTracking => {
                if let Some(project) = self.catalog.project().cloned() {
                    let tx = sender.input_sender().clone();
                    relm4::spawn(async move {
                        if let Err(error) = client::task_action(client::TaskAction::Clear {
                            project: project.id,
                        })
                        .await
                        {
                            eprintln!("[remarked] stop tracking failed: {error}");
                        }
                        let _ = tx.send(RemarkedInput::ReloadNote);
                    });
                }
            }
            RemarkedInput::SetTaskStatus(status) => {
                if let Some(task) = self.selected_task_info().cloned() {
                    let tx = sender.input_sender().clone();
                    relm4::spawn(async move {
                        let action = client::TaskAction::Status {
                            key: task.key,
                            status,
                        };
                        if let Err(error) = client::task_action(action).await {
                            eprintln!("[remarked] status change failed: {error}");
                        }
                        let _ = tx.send(RemarkedInput::ReloadNote);
                    });
                }
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

fn build_task_pane(
    sender: &ComponentSender<RemarkedWindow>,
) -> (
    adw::OverlaySplitView,
    gtk::ListBox,
    gtk::Button,
    gtk::Label,
    gtk::Label,
    gtk::Button,
    gtk::DropDown,
    gtk::Switch,
) {
    let split = adw::OverlaySplitView::builder()
        .sidebar_position(gtk::PackType::End)
        .show_sidebar(false)
        .pin_sidebar(true)
        .vexpand(true)
        .hexpand(false)
        .build();

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
    let add_button = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("New task")
        .css_classes(["flat", "circular"])
        .build();
    let details_button = gtk::Button::builder()
        .icon_name("document-properties-symbolic")
        .tooltip_text("Show task details")
        .css_classes(["flat", "circular"])
        .build();
    header.append(&title);
    header.append(&add_button);
    header.append(&details_button);
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

    let stop_button = gtk::Button::builder()
        .label("Stop tracking")
        .halign(gtk::Align::Fill)
        .css_classes(["destructive-action"])
        .build();
    let tx = sender.input_sender().clone();
    stop_button.connect_clicked(move |_| {
        let _ = tx.send(RemarkedInput::StopTracking);
    });
    content.append(&stop_button);

    let details = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .width_request(240)
        .vexpand(true)
        .build();
    details.add_css_class("remarked-task-details");

    let task_title = gtk::Label::builder()
        .halign(gtk::Align::Start)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::Word)
        .css_classes(["heading"])
        .build();
    details.append(&task_title);

    let task_description = gtk::Label::builder()
        .halign(gtk::Align::Start)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::Word)
        .css_classes(["dim-label", "caption"])
        .build();
    details.append(&task_description);

    let actions = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .build();
    let task_start_button = gtk::Button::builder()
        .label("Start")
        .css_classes(["suggested-action"])
        .build();
    let tx = sender.input_sender().clone();
    task_start_button.connect_clicked(move |_| {
        let _ = tx.send(RemarkedInput::StartTask);
    });
    actions.append(&task_start_button);

    let statuses = gtk::StringList::new(&[
        "planned",
        "in-progress",
        "completed",
        "deferred",
        "cancelled",
    ]);
    let task_status_drop = gtk::DropDown::builder().model(&statuses).build();
    let tx = sender.input_sender().clone();
    task_status_drop.connect_selected_notify(move |drop| {
        if let Some(status) = drop.selected_item().and_downcast::<gtk::StringObject>() {
            let _ = tx.send(RemarkedInput::SetTaskStatus(status.string().to_string()));
        }
    });
    actions.append(&task_status_drop);
    details.append(&actions);

    split.set_content(Some(&content));
    split.set_sidebar(Some(&details));

    let tx = sender.input_sender().clone();
    details_button.connect_clicked(move |_| {
        let _ = tx.send(RemarkedInput::ToggleTaskDetails);
    });

    (
        split,
        list_box,
        details_button,
        task_title,
        task_description,
        task_start_button,
        task_status_drop,
        show_finished_switch,
    )
}

fn build_note_pane(
    sender: &ComponentSender<RemarkedWindow>,
) -> (
    adw::OverlaySplitView,
    gtk::ListBox,
    gtk::Entry,
    gtk::Box,
    sourceview5::View,
    gtk::Entry,
    gtk::Button,
    adw::StatusPage,
    gtk::Button,
) {
    let split = adw::OverlaySplitView::builder()
        .sidebar_position(gtk::PackType::Start)
        .show_sidebar(true)
        .pin_sidebar(true)
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

    split.set_sidebar(Some(&sidebar));

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
    let source_view = sourceview5::View::with_buffer(&buffer);
    source_view.set_vexpand(true);
    source_view.set_hexpand(true);
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

    split.set_content(Some(&content));

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

fn load_styles(window: &adw::ApplicationWindow) {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(
        "
        .remarked-window {
            background: transparent;
        }
        .remarked-root {
            background: transparent;
        }
        .remarked-pane {
            background-color: rgba(30, 30, 30, 0.96);
            padding: 12px;
        }
        .remarked-task-pane {
            border-right: 1px solid rgba(255, 255, 255, 0.08);
        }
        .remarked-task-row {
            background: transparent;
            border-radius: 8px;
        }
        .remarked-task-row:hover {
            background: rgba(255, 255, 255, 0.05);
        }
        .remarked-task-row:active {
            background: rgba(255, 255, 255, 0.08);
        }
        .remarked-task-details {
            background: rgba(255, 255, 255, 0.05);
            border-radius: 16px;
            padding: 16px;
        }
        .remarked-row {
            background: rgba(255, 255, 255, 0.05);
            border-radius: 12px;
            margin: 4px 0;
        }
        .remarked-note-active {
            background: rgba(120, 174, 237, 0.15);
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
