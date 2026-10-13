use super::{
    actions::{self, Action},
    source::{GoalBarVm, GoalRow},
};
use adw::prelude::*;
use shell_core::{
    gtk,
    gtk4_layer_shell::{KeyboardMode, LayerShell},
};
use std::{cell::RefCell, rc::Rc};

pub(in crate::widgets::bar) struct GoalPopup {
    popover: gtk::Popover,
    rows: gtk::ListBox,
    overlay: adw::ToastOverlay,
    empty: gtk::Label,
    current: Rc<RefCell<GoalBarVm>>,
}
impl GoalPopup {
    pub fn new(button: &gtk::MenuButton) -> Self {
        let popover = gtk::Popover::new();
        let overlay = adw::ToastOverlay::new();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        content.add_css_class("goal-checklist");
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        let heading = gtk::Label::new(Some("Project goals"));
        heading.add_css_class("heading");
        content.append(&heading);
        let rows = gtk::ListBox::new();
        rows.set_selection_mode(gtk::SelectionMode::None);
        rows.add_css_class("boxed-list");
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_propagate_natural_height(true);
        scroll.set_child(Some(&rows));
        content.append(&scroll);
        let empty = gtk::Label::new(Some("No goals yet. Create one with proj goal add."));
        empty.add_css_class("dim-label");
        content.append(&empty);
        let clear = gtk::Button::with_label("Stop tracking");
        clear.add_css_class("flat");
        content.append(&clear);
        let current = Rc::new(RefCell::new(GoalBarVm::default()));
        let target = current.clone();
        let toast = overlay.clone();
        clear.connect_clicked(move |_| {
            if let Some(project) = target.borrow().project_id.clone() {
                actions::run(
                    Action::Track {
                        project,
                        key: String::new(),
                    },
                    &toast,
                    |_| {},
                );
            }
        });
        overlay.set_child(Some(&content));
        popover.set_child(Some(&overlay));
        button.set_popover(Some(&popover));
        popover.connect_visible_notify(move |p| {
            if let Some(window) = p.root().and_downcast::<gtk::Window>() {
                window.set_keyboard_mode(if p.is_visible() {
                    KeyboardMode::Exclusive
                } else {
                    KeyboardMode::None
                });
                if let Some(surface) = window.surface() {
                    if let Some(monitor) = surface.display().monitor_at_surface(&surface) {
                        scroll.set_max_content_height(monitor.geometry().height() / 3);
                    }
                }
            }
        });
        Self {
            popover,
            rows,
            overlay,
            empty,
            current,
        }
    }
    pub fn update(&self, vm: &GoalBarVm) {
        if *self.current.borrow() == *vm {
            return;
        }
        if self.current.borrow().project_id != vm.project_id {
            self.popover.popdown();
        }
        *self.current.borrow_mut() = vm.clone();
        while let Some(child) = self.rows.first_child() {
            self.rows.remove(&child);
        }
        self.empty.set_visible(vm.rows.is_empty());
        for row in &vm.rows {
            self.rows.append(&self.row(row, vm));
        }
    }
    fn row(&self, row: &GoalRow, vm: &GoalBarVm) -> adw::ActionRow {
        let goal = &row.goal;
        let item = adw::ActionRow::new();
        item.set_use_markup(false);
        item.set_title(&goal.title);
        item.set_title_lines(2);
        item.set_subtitle_lines(2);
        let status = match goal.status.as_str() {
            "in-progress" => "In progress",
            "completed" => "Completed",
            "deferred" => "Deferred",
            _ => "To do",
        };
        item.set_subtitle(&if goal.description.is_empty() {
            status.into()
        } else {
            format!("{status} · {}", goal.description)
        });
        item.set_margin_start((row.depth.min(10) * 16) as i32);
        let check = gtk::CheckButton::new();
        check.set_valign(gtk::Align::Center);
        check.set_active(goal.status == "completed");
        check.update_property(&[gtk::accessible::Property::Label(&format!(
            "Complete {}",
            goal.title
        ))]);
        let key = goal.key();
        let overlay = self.overlay.clone();
        check.connect_toggled(move |check| {
            if !check.is_sensitive() {
                return;
            }
            let completed = check.is_active();
            check.set_sensitive(false);
            let weak = check.downgrade();
            actions::run(
                Action::Status {
                    key: key.clone(),
                    status: if completed { "completed" } else { "planned" }.into(),
                },
                &overlay,
                move |ok| {
                    if let Some(check) = weak.upgrade() {
                        if !ok {
                            check.set_active(!completed);
                        }
                        check.set_sensitive(true);
                    }
                },
            );
        });
        item.add_prefix(&check);
        let track = gtk::Button::from_icon_name("media-playback-start-symbolic");
        track.add_css_class("flat");
        track.set_valign(gtk::Align::Center);
        let active = vm.active == goal.key();
        track.set_tooltip_text(Some(if active {
            "Currently tracked"
        } else {
            "Track this goal on the bar"
        }));
        track.set_sensitive(!active && !matches!(goal.status.as_str(), "completed" | "deferred"));
        if active {
            track.add_css_class("accent");
        }
        let project = vm.project_id.clone().unwrap_or_default();
        let key = goal.key();
        let overlay = self.overlay.clone();
        track.connect_clicked(move |_| {
            actions::run(
                Action::Track {
                    project: project.clone(),
                    key: key.clone(),
                },
                &overlay,
                |_| {},
            )
        });
        item.add_suffix(&track);
        item.set_activatable(true);
        item.set_tooltip_text(Some("Click the title to open its Markdown note in $EDITOR"));
        let key = goal.key();
        let overlay = self.overlay.clone();
        let popup = self.popover.downgrade();
        item.connect_activated(move |_| {
            if let Some(p) = popup.upgrade() {
                p.popdown();
            }
            actions::open_note(key.clone(), &overlay);
        });
        item
    }
}
