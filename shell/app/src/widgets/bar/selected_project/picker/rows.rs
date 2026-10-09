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
        "Remove metadata for “{}” and its {} registered checkouts",
        project.name,
        project.checkouts.len()
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
fn select_button(
    project: &ProjectChoice,
    checkout: &CheckoutChoice,
    state: &Rc<PickerState>,
) -> gtk::Button {
    let select = icon_button(
        "object-select-symbolic",
        &format!("Select project {} — {}", project.name, checkout.path),
    );
    let selected = state
        .target
        .borrow()
        .project
        .as_ref()
        .and_then(|p| p.path.as_ref())
        == Some(&checkout.path);
    if selected {
        select.add_css_class("accent");
        select.set_sensitive(false);
        select.set_tooltip_text(Some("Current project checkout"));
    }
    let workspace = state.target.borrow().workspace_id;
    let weak = Rc::downgrade(state);
    let path = checkout.path.clone();
    select.connect_clicked(move |_| {
        if let (Some(state), Some(workspace)) = (weak.upgrade(), workspace) {
            state.chooser_popover.popdown();
            state.actions.run(Action::Assign {
                workspace,
                path: path.clone(),
            });
        }
    });
    select
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
    row.set_subtitle(&format!("{} · {}", checkout.branch, checkout.path));
    row.set_title_lines(1);
    row.set_subtitle_lines(1);
    row.add_suffix(&select_button(project, checkout, state));
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
        if let Some(checkout) = matching
            .iter()
            .find(|c| c.path == project.path)
            .or_else(|| matching.first())
        {
            row.add_suffix(&select_button(project, checkout, state));
        }
        row.add_suffix(&remove_button(project, &state.actions));
        for checkout in matching {
            row.add_row(&checkout_row(project, &checkout, state));
        }
        row.set_expanded(!query.trim().is_empty());
        state.results.append(&row);
    }
}
