use locus::{RelationEndpoint, RelationRecord, keys};

use super::super::IconChoice;

const ICON_GLYPH_KIND: &str = "org.rsynapse.icon.glyph";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorkspaceIconIdentity {
    pub(super) subjects: Vec<RelationEndpoint>,
}

pub(super) fn icon_target(glyph: String) -> RelationEndpoint {
    RelationEndpoint::stable_key(ICON_GLYPH_KIND, glyph)
}

pub(super) fn icon_choice_from_record(record: &RelationRecord) -> Option<IconChoice> {
    let RelationEndpoint::StableKey { kind, id } = &record.target else {
        return None;
    };
    (kind == ICON_GLYPH_KIND)
        .then(|| non_empty(id))
        .flatten()
        .and_then(|glyph| IconChoice::new(glyph.to_owned()))
}

#[cfg(test)]
pub(in crate::widgets::bar) fn workspace_icon_subjects_for_test(
    id: u64,
    name: Option<&str>,
) -> Vec<RelationEndpoint> {
    WorkspaceIconIdentity::new(id, name).subjects
}

impl WorkspaceIconIdentity {
    pub(super) fn new(id: u64, name: Option<&str>) -> Self {
        let id_subject = workspace_id_subject(id);
        let subjects = match name.and_then(non_empty) {
            Some(name) => vec![workspace_name_subject(name), id_subject],
            None => vec![id_subject],
        };
        Self { subjects }
    }

    pub(super) fn primary(&self) -> &RelationEndpoint {
        &self.subjects[0]
    }
}

fn workspace_id_subject(id: u64) -> RelationEndpoint {
    RelationEndpoint::stable_key(keys::NIRI_WORKSPACE_ID, id.to_string())
}

fn workspace_name_subject(name: &str) -> RelationEndpoint {
    RelationEndpoint::stable_key(keys::NIRI_WORKSPACE_NAME, name)
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}
