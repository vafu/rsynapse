//! Reactive task list row. Structural task data comes from the parent list;
//! live task properties subscribe here.
use adw::prelude::*;
use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::*},
    source::{Observable, StateSignal, rx::Observable as _},
};

use crate::{model, source::TagCounts};

const TASK_STATUSES: [&str; 5] = [
    "planned",
    "in-progress",
    "completed",
    "deferred",
    "cancelled",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TaskRowInit {
    pub instance_id: String,
    pub key: String,
    pub project_id: String,
    pub depth: usize,
    pub has_children: bool,
    pub collapsed: bool,
    pub tag_counts: StateSignal<TagCounts>,
}

impl TaskRowInit {
    pub(crate) fn row_key(&self) -> String {
        self.instance_id.clone()
    }
}

#[derive(Debug)]
pub(crate) enum TaskRowInput {
    Source(Box<task_row_sources::Msg>),
    ToggleCollapse,
    SetStatus(String),
    Start,
    Stop,
}

impl From<task_row_sources::Msg> for TaskRowInput {
    fn from(message: task_row_sources::Msg) -> Self {
        Self::Source(Box::new(message))
    }
}

#[derive(Debug)]
pub enum TaskRowOutput {
    ToggleCollapse(String),
    SetStatus { key: String, status: String },
    Start { key: String, project: String },
    Stop,
}

#[shell_macros::model(module = task_row_sources)]
pub(crate) struct TaskRow {
    instance_id: String,
    key: String,
    project_id: String,
    depth: usize,
    has_children: bool,
    tag_counts: StateSignal<TagCounts>,
    collapsed: bool,
    status_button: gtk::MenuButton,
    toggle_button: gtk::ToggleButton,

    #[source(shell_core::source::proj::goal_title(key.clone()))]
    title: String,

    #[source(shell_core::source::proj::goal_status(key.clone()))]
    status: String,

    #[source(shell_core::source::proj::goal_tracked(instance_id.clone()))]
    active: bool,

    #[source(task_note_count(tag_counts.observable(), instance_id.clone()))]
    note_count: u32,
}

#[shell_macros::component(module = task_row_sources, model = TaskRow)]
#[relm4::component(pub(crate))]
impl SimpleComponent for TaskRow {
    type Init = TaskRowInit;
    type Input = TaskRowInput;
    type Output = TaskRowOutput;

    view! {
        #[root]
        adw::ActionRow {
            set_activatable: true,
            set_widget_name: model.instance_id.as_str(),
            set_title_lines: 1,
            set_subtitle_lines: 1,
            #[watch]
            set_margin_start: model.depth as i32 * 14,
            #[watch]
            set_title: model.title.as_str(),
            #[watch]
            set_subtitle: note_count_text(model.note_count).as_str(),
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let collapsed = init.collapsed;
        let mut model = TaskRow::new(
            init.instance_id,
            init.key,
            init.project_id,
            init.depth,
            init.has_children,
            init.tag_counts,
            collapsed,
            gtk::MenuButton::new(),
            gtk::ToggleButton::new(),
        );
        let widgets = view_output!();
        let status_button = gtk::MenuButton::builder()
            .icon_name(status_icon(&model.status, model.active))
            .tooltip_text("Task status and tracking")
            .valign(gtk::Align::Center)
            .css_classes(["flat", "circular", "remarked-task-status"])
            .build();
        root.add_prefix(&status_button);
        let toggle_button = gtk::ToggleButton::builder()
            .icon_name("pan-end-symbolic")
            .active(!model.collapsed)
            .tooltip_text("Toggle subtasks")
            .valign(gtk::Align::Center)
            .css_classes(["flat", "circular", "remarked-task-toggle"])
            .visible(model.has_children)
            .build();
        let sender_for_toggle = sender.clone();
        toggle_button
            .connect_toggled(move |_| sender_for_toggle.input(TaskRowInput::ToggleCollapse));
        root.add_suffix(&toggle_button);
        model.status_button = status_button.clone();
        model.toggle_button = toggle_button.clone();
        build_status_menu(&status_button, &sender);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            TaskRowInput::Source(message) => {
                TaskRow::update(self, *message);
                self.status_button
                    .set_icon_name(status_icon(&self.status, self.active));
            }
            TaskRowInput::ToggleCollapse => {
                self.collapsed = !self.toggle_button.is_active();
                let _ = sender.output(TaskRowOutput::ToggleCollapse(self.key.clone()));
            }
            TaskRowInput::SetStatus(status) => {
                let _ = sender.output(TaskRowOutput::SetStatus {
                    key: self.key.clone(),
                    status,
                });
            }
            TaskRowInput::Start => {
                let _ = sender.output(TaskRowOutput::Start {
                    key: self.key.clone(),
                    project: self.project_id.clone(),
                });
            }
            TaskRowInput::Stop => {
                let _ = sender.output(TaskRowOutput::Stop);
            }
        }
    }
}

fn build_status_menu(button: &gtk::MenuButton, sender: &ComponentSender<TaskRow>) {
    use gtk::glib::variant::ToVariant;

    let actions = gtk::gio::SimpleActionGroup::new();

    let status_action = gtk::gio::SimpleAction::new_stateful(
        "status",
        Some(gtk::glib::VariantTy::STRING),
        &"planned".to_variant(),
    );
    let tx = sender.input_sender().clone();
    status_action.connect_activate(move |action, parameter| {
        let Some(parameter) = parameter else {
            return;
        };
        let Some(status) = parameter.str() else {
            return;
        };
        action.set_state(parameter);
        let _ = tx.send(TaskRowInput::SetStatus(status.to_string()));
    });
    actions.add_action(&status_action);

    let start_action = gtk::gio::SimpleAction::new("start", None);
    let tx = sender.input_sender().clone();
    start_action.connect_activate(move |_, _| {
        let _ = tx.send(TaskRowInput::Start);
    });
    actions.add_action(&start_action);

    let stop_action = gtk::gio::SimpleAction::new("stop", None);
    let tx = sender.input_sender().clone();
    stop_action.connect_activate(move |_, _| {
        let _ = tx.send(TaskRowInput::Stop);
    });
    actions.add_action(&stop_action);

    let status_menu = gtk::gio::Menu::new();
    for status in TASK_STATUSES {
        let item = gtk::gio::MenuItem::new(Some(&status.replace('-', " ")), None);
        item.set_action_and_target_value(Some("task.status"), Some(&status.to_variant()));
        status_menu.append_item(&item);
    }

    let tracking = gtk::gio::Menu::new();
    tracking.append(Some("Start tracking"), Some("task.start"));
    tracking.append(Some("Stop tracking"), Some("task.stop"));

    let menu = gtk::gio::Menu::new();
    menu.append_section(None, &status_menu);
    menu.append_section(None, &tracking);

    button.set_menu_model(Some(&menu));
    button.insert_action_group("task", Some(&actions));
}

fn task_note_count(tag_counts: Observable<TagCounts>, instance_id: String) -> Observable<u32> {
    let tag = model::task_tag(&instance_id);
    tag_counts
        .map(move |counts| counts.count(&tag).unwrap_or(0))
        .distinct_until_changed()
        .box_it()
}

fn note_count_text(count: u32) -> String {
    match count {
        1 => "1 note".to_string(),
        0 => String::new(),
        count => format!("{count} notes"),
    }
}

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
