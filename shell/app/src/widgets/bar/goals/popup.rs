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
mod collapse;
mod rows;
use collapse::CollapseState;
use rows::RenderedRow;

pub(in crate::widgets::bar) struct GoalPopup {
    popover: gtk::Popover,
    rows: gtk::ListBox,
    overlay: adw::ToastOverlay,
    empty: gtk::Label,
    current: Rc<RefCell<GoalBarVm>>,
    folds: Rc<RefCell<CollapseState>>,
    rendered: Rc<RefCell<Vec<RenderedRow>>>,
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
                    // Exclusive layer focus would dismiss an existing popup grab.
                    KeyboardMode::OnDemand
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
            folds: Rc::new(RefCell::new(CollapseState::default())),
            rendered: Rc::new(RefCell::new(Vec::new())),
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
        self.folds.borrow_mut().sync(&vm.rows);
        self.rendered.borrow_mut().clear();
        while let Some(child) = self.rows.first_child() {
            self.rows.remove(&child);
        }
        self.empty.set_visible(vm.rows.is_empty());
        for row in &vm.rows {
            let rendered = self.row(row);
            self.rows.append(&rendered.item);
            self.rendered.borrow_mut().push(rendered);
        }
        apply_folds(
            &self.current.borrow(),
            &self.folds.borrow(),
            &self.rendered.borrow(),
        );
    }
}

fn apply_folds(vm: &GoalBarVm, folds: &CollapseState, rendered: &[RenderedRow]) {
    for ((model, row), visible) in vm.rows.iter().zip(rendered).zip(folds.visibility(&vm.rows)) {
        row.item.set_visible(visible);
        if let Some(button) = &row.disclosure {
            let expanded = folds.expanded(model);
            button.set_icon_name(if expanded {
                "pan-down-symbolic"
            } else {
                "pan-end-symbolic"
            });
            button.set_tooltip_text(Some(&format!(
                "{} subgoals ({}/{})",
                if expanded { "Hide" } else { "Show" },
                model.completed_children,
                model.child_count
            )));
            button.update_state(&[gtk::accessible::State::Expanded(Some(expanded))]);
        }
    }
}
