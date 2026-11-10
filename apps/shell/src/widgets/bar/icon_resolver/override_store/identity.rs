pub(super) use data_sources::workspace_icon::WorkspaceIconIdentity;
use locus::RelationEndpoint;

use super::super::IconChoice;

pub(super) fn icon_choice_from_record(record: &locus::RelationRecord) -> Option<IconChoice> {
    data_sources::workspace_icon::glyph_from_record(record).and_then(IconChoice::new)
}

#[cfg(test)]
pub(in crate::widgets::bar) fn workspace_icon_subjects_for_test(
    id: impl ToString,
    name: Option<&str>,
) -> Vec<RelationEndpoint> {
    WorkspaceIconIdentity::new(id.to_string(), name).subjects
}
