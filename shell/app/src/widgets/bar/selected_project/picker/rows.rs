use super::{
    PickerState,
    actions::{Action, Actions, icon_button},
    choices::{CheckoutChoice, ProjectChoice},
};
use adw::prelude::*;
use shell_core::gtk;
use std::rc::Rc;

pub(super) fn remove_button(project: &ProjectChoice, actions: &Rc<Actions>) -> gtk::Button {
    let remove = icon_button(
        "user-trash-symbolic",
        &format!("Remove project {}", project.name),
    );
    remove.set_tooltip_text(Some(&format!(
        "Remove project metadata for “{}”",
        project.name
    )));
    let weak = Rc::downgrade(actions);
    let id = project.id.clone();
    let name = project.name.clone();
    remove.connect_clicked(move |_| {
        if let Some(actions) = weak.upgrade() {
            actions.run(Action::Remove {
                id: id.clone(),
                name: name.clone(),
            });
        }
    });
    remove
}
fn checkout_row(
    project: &ProjectChoice,
    checkout: &CheckoutChoice,
    state: &Rc<PickerState>,
) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_use_markup(false);
    row.set_title(
        std::path::Path::new(&checkout.path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&project.name),
    );
    row.set_subtitle(&if checkout.branch.is_empty() {
        checkout.path.clone()
    } else {
        format!("{} · {}", checkout.branch, checkout.path)
    });
    row.set_title_lines(1);
    row.set_subtitle_lines(1);
    row.set_activatable(true);
    let selected = state
        .target
        .borrow()
        .project
        .as_ref()
        .and_then(|p| p.project_id.as_ref())
        == Some(&project.id);
    if selected {
        let badge = gtk::Label::new(Some("Current"));
        badge.add_css_class("dim-label");
        row.add_suffix(&badge);
    }
    let workspace = state.target.borrow().workspace_id;
    let weak = Rc::downgrade(state);
    let path = checkout.path.clone();
    let project_id = project.id.clone();
    row.connect_activated(move |_| {
        if let (Some(state), Some(workspace)) = (weak.upgrade(), workspace) {
            state.chooser_popover.popdown();
            let current = state
                .target
                .borrow()
                .project
                .as_ref()
                .and_then(|p| p.project_id.as_ref())
                == Some(&project_id);
            if !current {
                state.actions.run(Action::Assign {
                    workspace,
                    path: path.clone(),
                });
            }
        }
    });
    row
}
pub(super) fn catalog_row(project: &ProjectChoice, state: &Rc<PickerState>, query: &str) {
    if project.checkouts.len() == 1 {
        let row = checkout_row(project, &project.checkouts[0], state);
        row.set_title(&project.name);
        row.add_suffix(&remove_button(project, &state.actions));
        state.results.append(&row);
    } else {
        let row = adw::ExpanderRow::new();
        row.set_use_markup(false);
        row.set_title(&project.name);
        row.set_subtitle(&format!(
            "{} checkouts · {}",
            project.checkouts.len(),
            project.path
        ));
        row.set_subtitle_lines(1);
        let matching = project.matching(query);
        row.add_suffix(&remove_button(project, &state.actions));
        for checkout in matching {
            row.add_row(&checkout_row(project, &checkout, state));
        }
        row.set_expanded(!query.trim().is_empty());
        state.results.append(&row);
    }
}
