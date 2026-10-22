use super::super::actions::{Action, icon_button};
use super::{
    State,
    model::{Target, new_label},
};
use adw::prelude::*;
use std::rc::Rc;

impl State {
    fn is_current(&self, target: &Target) -> bool {
        self.target.borrow().as_ref() == Some(target)
    }

    pub(super) fn receive(self: &Rc<Self>, target: &Target, labels: Option<Vec<String>>) {
        if !self.is_current(target) {
            return;
        }
        *self.labels.borrow_mut() = labels;
        self.render();
    }

    pub(super) fn update_controls(&self) {
        let labels = self.labels.borrow();
        let enabled = self.target.borrow().is_some() && labels.is_some() && !self.busy.get();
        self.entry.set_sensitive(enabled);
        self.list.set_sensitive(enabled);
        self.add.set_sensitive(
            enabled && new_label(&self.entry.text(), labels.as_deref(), self.busy.get()).is_some(),
        );
    }

    pub(super) fn render(self: &Rc<Self>) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let labels = self.labels.borrow().clone();
        self.status.set_text(match &labels {
            None => "Labels loading or unavailable",
            Some(labels) if labels.is_empty() => "No labels — showing all GitHub PRs",
            Some(_) => "",
        });
        self.status
            .set_visible(labels.as_ref().is_none_or(|labels| labels.is_empty()));
        self.list
            .set_visible(labels.as_ref().is_some_and(|labels| !labels.is_empty()));
        if let (Some(target), Some(labels)) = (self.target.borrow().clone(), labels) {
            for label in labels {
                let row = adw::ActionRow::new();
                row.set_use_markup(false);
                row.set_title(&label);
                let remove =
                    icon_button("list-remove-symbolic", &format!("Remove label “{label}”"));
                let weak = Rc::downgrade(self);
                let captured = target.clone();
                remove.connect_clicked(move |_| {
                    if let Some(state) = weak.upgrade() {
                        state.remove_label(&captured, &label);
                    }
                });
                row.add_suffix(&remove);
                self.list.append(&row);
            }
        }
        self.update_controls();
    }

    pub(super) fn add_label(self: &Rc<Self>) {
        let Some(target) = self.target.borrow().clone() else {
            return;
        };
        let text = self.entry.text().to_string();
        let Some(label) = new_label(&text, self.labels.borrow().as_deref(), self.busy.get()) else {
            return;
        };
        self.run(
            target.clone(),
            Action::AddLabel {
                id: target.project,
                label,
            },
            Some(text),
        );
    }

    pub(super) fn remove_label(self: &Rc<Self>, target: &Target, label: &str) {
        if !self.is_current(target)
            || self.busy.get()
            || !self
                .labels
                .borrow()
                .as_ref()
                .is_some_and(|labels| labels.iter().any(|l| l == label))
        {
            return;
        }
        self.run(
            target.clone(),
            Action::RemoveLabel {
                id: target.project.clone(),
                label: label.to_owned(),
            },
            None,
        );
    }

    fn run(self: &Rc<Self>, target: Target, action: Action, input: Option<String>) {
        self.busy.set(true);
        self.update_controls();
        let weak = Rc::downgrade(self);
        let task = self.actions.run_scoped(action, move |success| {
            weak.upgrade()
                .is_some_and(|state| state.finish(&target, input.as_deref(), success))
        });
        *self.command.borrow_mut() = task;
    }

    pub(super) fn finish(&self, target: &Target, input: Option<&str>, success: bool) -> bool {
        if !self.is_current(target) {
            return false;
        }
        self.busy.set(false);
        if success && input.is_some_and(|input| self.entry.text().as_str() == input) {
            self.entry.set_text("");
        }
        // Do not apply the command response as a list snapshot: a concurrent
        // CLI/agent edit may already have advanced the authoritative property.
        self.update_controls();
        true
    }
}
