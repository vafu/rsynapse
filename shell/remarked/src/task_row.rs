//! Reactive task list row. Structural task data comes from the parent list;
//! live task properties subscribe here.
use adw::prelude::*;
use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::WidgetExt},
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
    RefreshTheme,
    ToggleCollapse,
    SetStatus(String),
    Start,
    Stop,
    Delete,
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
    Delete { key: String },
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
    actions_menu: gtk::PopoverMenu,
    start_action: gtk::gio::SimpleAction,
    stop_action: gtk::gio::SimpleAction,
    priority_colors: PriorityColors,

    #[source(shell_core::source::proj::goal_title(key.clone()))]
    title: String,

    #[source(shell_core::source::proj::goal_status(key.clone()))]
    status: String,

    #[source(shell_core::source::proj::goal_priority(key.clone()))]
    priority: String,

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
            set_use_markup: true,
            #[watch]
            set_margin_start: model.depth as i32 * 14,
            #[watch]
            set_title: model.title.as_str(),
            #[watch]
            set_subtitle: task_subtitle(model.priority.as_str(), model.note_count, &model.priority_colors).as_str(),
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
            gtk::PopoverMenu::from_model(None::<&gtk::gio::MenuModel>),
            gtk::gio::SimpleAction::new("start", None),
            gtk::gio::SimpleAction::new("stop", None),
            PriorityColors::default(),
        );
        let widgets = view_output!();
        let status_button = gtk::MenuButton::builder()
            .icon_name(status_icon(&model.status, model.active))
            .tooltip_text("Task status")
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
        model.priority_colors = PriorityColors::resolve(&root);
        let theme_sender = sender.clone();
        adw::StyleManager::default().connect_dark_notify(move |_| {
            theme_sender.input(TaskRowInput::RefreshTheme);
        });
        build_status_menu(&status_button, &sender);
        let (actions_menu, start_action, stop_action) = build_actions_menu(&root, &sender);
        model.actions_menu = actions_menu;
        model.start_action = start_action;
        model.stop_action = stop_action;
        model.sync_actions();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            TaskRowInput::Source(message) => {
                TaskRow::update(self, *message);
                self.status_button
                    .set_icon_name(status_icon(&self.status, self.active));
                self.sync_actions();
            }
            TaskRowInput::RefreshTheme => {
                self.priority_colors = PriorityColors::resolve(&self.status_button);
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
            TaskRowInput::Delete => {
                let _ = sender.output(TaskRowOutput::Delete {
                    key: self.key.clone(),
                });
            }
        }
    }
}

impl TaskRow {
    fn sync_actions(&self) {
        self.start_action.set_enabled(!self.active);
        self.stop_action.set_enabled(self.active);
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

    let status_menu = gtk::gio::Menu::new();
    for status in TASK_STATUSES {
        let item = gtk::gio::MenuItem::new(Some(&status.replace('-', " ")), None);
        item.set_action_and_target_value(Some("task.status"), Some(&status.to_variant()));
        status_menu.append_item(&item);
    }

    let menu = gtk::gio::Menu::new();
    menu.append_section(None, &status_menu);

    button.set_menu_model(Some(&menu));
    button.insert_action_group("task", Some(&actions));
}

/// Builds the per-task context menu opened by right-clicking the row. Returns
/// the tracking actions so the row can reflect whether it is currently tracked.
fn build_actions_menu(
    row: &adw::ActionRow,
    sender: &ComponentSender<TaskRow>,
) -> (
    gtk::PopoverMenu,
    gtk::gio::SimpleAction,
    gtk::gio::SimpleAction,
) {
    let actions = gtk::gio::SimpleActionGroup::new();

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

    let delete_action = gtk::gio::SimpleAction::new("delete", None);
    let delete_row = row.clone();
    let tx = sender.input_sender().clone();
    delete_action.connect_activate(move |_, _| confirm_delete(&delete_row, &tx));
    actions.add_action(&delete_action);

    let tracking = gtk::gio::Menu::new();
    tracking.append(Some("Start tracking"), Some("task.start"));
    tracking.append(Some("Stop tracking"), Some("task.stop"));

    let delete = gtk::gio::Menu::new();
    delete.append(Some("Delete Task…"), Some("task.delete"));

    let menu = gtk::gio::Menu::new();
    menu.append_section(None, &tracking);
    menu.append_section(None, &delete);

    let popover = gtk::PopoverMenu::from_model(Some(&menu));
    popover.set_has_arrow(false);
    popover.set_parent(row);
    popover.insert_action_group("task", Some(&actions));
    let popover_for_destroy = popover.clone();
    row.connect_destroy(move |_| popover_for_destroy.unparent());

    let secondary = gtk::GestureClick::new();
    secondary.set_button(gtk::gdk::BUTTON_SECONDARY);
    secondary.set_propagation_phase(gtk::PropagationPhase::Capture);
    secondary.connect_pressed(|gesture, _, _, _| {
        let _ = gesture.set_state(gtk::EventSequenceState::Claimed);
    });
    let popover_for_click = popover.clone();
    secondary.connect_released(move |_, _, x, y| {
        popover_for_click
            .set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover_for_click.popup();
    });
    row.add_controller(secondary);

    (popover, start_action, stop_action)
}

fn confirm_delete(row: &adw::ActionRow, tx: &relm4::Sender<TaskRowInput>) {
    let dialog = adw::AlertDialog::builder()
        .heading("Delete task?")
        .body(
            "This removes the task and its history. Attached notes are kept. \
             Remove subtasks before deleting their parent.",
        )
        .build();
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("delete", "Delete");
    dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    let tx = tx.clone();
    dialog.connect_response(None, move |_, response| {
        if response == "delete" {
            let _ = tx.send(TaskRowInput::Delete);
        }
    });
    let window = row.root().and_downcast::<gtk::Window>();
    dialog.present(window.as_ref());
}

fn task_note_count(tag_counts: Observable<TagCounts>, instance_id: String) -> Observable<u32> {
    let tag = model::task_tag(&instance_id);
    tag_counts
        .map(move |counts| counts.count(&tag).unwrap_or(0))
        .distinct_until_changed()
        .box_it()
}

#[derive(Clone, Debug, Default)]
struct PriorityColors {
    high: String,
    medium: String,
    low: String,
}

impl PriorityColors {
    // GTK 4.10 deprecated `style_context()`/`lookup_color()` without a
    // replacement for resolving named `@define-color` theme values. Priority
    // text is rendered through Pango markup, which cannot reference CSS classes,
    // so this is the only way to keep it bound to the active Adwaita theme.
    #[allow(deprecated)]
    fn resolve(widget: &impl IsA<gtk::Widget>) -> Self {
        let context = widget.style_context();
        let to_hex = |color: gtk::gdk::RGBA| {
            format!(
                "#{:02x}{:02x}{:02x}",
                (color.red() * 255.0).round() as u8,
                (color.green() * 255.0).round() as u8,
                (color.blue() * 255.0).round() as u8,
            )
        };
        let fallback = context.lookup_color("theme_fg_color").map(to_hex);
        let lookup = |name: &str| {
            context
                .lookup_color(name)
                .map(to_hex)
                .or_else(|| fallback.clone())
                .unwrap_or_default()
        };
        Self {
            high: lookup("error_color"),
            medium: lookup("warning_color"),
            low: context
                .lookup_color("insensitive_fg_color")
                .map(to_hex)
                .unwrap_or_default(),
        }
    }

    fn for_priority(&self, priority: &str) -> &str {
        match priority {
            "high" => &self.high,
            "low" => &self.low,
            _ => &self.medium,
        }
    }
}

fn task_subtitle(priority: &str, note_count: u32, colors: &PriorityColors) -> String {
    let (label, color) = match priority {
        "high" => ("High", colors.for_priority("high")),
        "low" => ("Low", colors.for_priority("low")),
        _ => ("Medium", colors.for_priority("medium")),
    };
    let badge = if color.is_empty() {
        format!(r#"<span weight="bold">{label}</span>"#)
    } else {
        format!(r#"<span foreground="{color}" weight="bold">{label}</span>"#)
    };
    match note_count {
        0 => badge,
        1 => format!("{badge}  ·  1 note"),
        count => format!("{badge}  ·  {count} notes"),
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
