mod identity;

use data_sources::{self, Observable, rx::Observable as _};

use super::IconChoice;
use crate::widgets::bar::workspace::WorkspaceObject;
use identity::{WorkspaceIconIdentity, icon_choice_from_record};

#[cfg(test)]
pub(in crate::widgets::bar) use identity::workspace_icon_subjects_for_test;

const WORKSPACE_ICON_OVERRIDE_RELATION: &str = data_sources::workspace_icon::OVERRIDE_RELATION;
pub(in crate::widgets::bar) fn workspace_icon_override_source(
    workspace: WorkspaceObject,
) -> Observable<Option<IconChoice>> {
    let identity = workspace
        .id()
        .combine_latest(workspace.name(), |id, name| {
            WorkspaceIconIdentity::new(id, name.as_deref())
        })
        .box_it();
    data_sources::switch_map(identity, locus_workspace_icon_override)
        .distinct_until_changed()
        .box_it()
}

pub(in crate::widgets::bar) fn set_workspace_icon_override(
    workspace_id: String,
    workspace_name: String,
    icon: IconChoice,
    picker_input: String,
) {
    data_sources::workspace_icon::set_override(
        workspace_id,
        workspace_name,
        icon.glyph,
        picker_input,
    );
}

pub(in crate::widgets::bar) fn clear_workspace_icon_override(
    workspace_id: String,
    workspace_name: String,
) {
    data_sources::workspace_icon::clear_override(workspace_id, workspace_name);
}

fn locus_workspace_icon_override(
    identity: WorkspaceIconIdentity,
) -> Observable<Option<IconChoice>> {
    let subjects = identity.subjects;
    let filter_subjects = subjects.clone();
    data_sources::watch_relation(subjects, WORKSPACE_ICON_OVERRIDE_RELATION)
        .map(move |records| {
            filter_subjects.iter().find_map(|subject| {
                records
                    .iter()
                    .find(|record| record.subject == *subject)
                    .and_then(icon_choice_from_record)
            })
        })
        .distinct_until_changed()
        .box_it()
}
