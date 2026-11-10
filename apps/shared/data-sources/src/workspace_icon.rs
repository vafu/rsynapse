//! Workspace icon override relations and icon-picker process access.
use std::collections::HashMap;

use locus::{RelationEndpoint, RelationRecord, keys};

use crate::{Observable, rx::Observable as _};

pub const OVERRIDE_RELATION: &str = "org.rsynapse.workspace.icon-override";
const ICON_GLYPH_KIND: &str = "org.rsynapse.icon.glyph";
const PICKER_INPUT_METADATA: &str = "pick-icon-input";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceIconIdentity {
    pub subjects: Vec<RelationEndpoint>,
}

impl WorkspaceIconIdentity {
    pub fn new(id: String, name: Option<&str>) -> Self {
        let id_subject = RelationEndpoint::stable_key(keys::WORKSPACE_ID, id);
        let subjects = match name.and_then(non_empty) {
            Some(name) => vec![
                RelationEndpoint::stable_key(keys::WORKSPACE_NAME, name),
                id_subject,
            ],
            None => vec![id_subject],
        };
        Self { subjects }
    }

    pub fn primary(&self) -> &RelationEndpoint {
        &self.subjects[0]
    }
}

pub fn icon_target(glyph: String) -> RelationEndpoint {
    RelationEndpoint::stable_key(ICON_GLYPH_KIND, glyph)
}

pub fn glyph_from_record(record: &RelationRecord) -> Option<String> {
    let RelationEndpoint::StableKey { kind, id } = &record.target else {
        return None;
    };
    (kind == ICON_GLYPH_KIND)
        .then(|| non_empty(id))
        .flatten()
        .map(str::to_owned)
}

/// Live icon glyph override for one workspace identity.
pub fn override_glyph(identity: WorkspaceIconIdentity) -> Observable<Option<String>> {
    let filter_subjects = identity.subjects.clone();
    crate::watch_relation(identity.subjects, OVERRIDE_RELATION)
        .map(move |records| {
            filter_subjects.iter().find_map(|subject| {
                records
                    .iter()
                    .find(|record| record.subject == *subject)
                    .and_then(glyph_from_record)
            })
        })
        .distinct_until_changed()
        .box_it()
}

pub fn set_override(
    workspace_id: String,
    workspace_name: String,
    glyph: String,
    picker_input: String,
) {
    crate::write::spawn("workspace icon override", async move {
        set_override_async(workspace_id, workspace_name, glyph, picker_input).await
    });
}

pub fn clear_override(workspace_id: String, workspace_name: String) {
    crate::write::spawn("workspace icon clear", async move {
        clear_override_async(workspace_id, workspace_name).await
    });
}

async fn set_override_async(
    workspace_id: String,
    workspace_name: String,
    glyph: String,
    picker_input: String,
) -> Result<(), String> {
    let glyph = non_empty(&glyph).ok_or_else(|| "empty icon override".to_string())?;
    let connection = zbus::Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    let proxy = locus::RelationsProxy::new(&connection)
        .await
        .map_err(|error| format!("connect locus proxy: {error}"))?;
    let identity = WorkspaceIconIdentity::new(workspace_id, Some(&workspace_name));
    let mut metadata = HashMap::new();
    if let Some(input) = non_empty(&picker_input) {
        metadata.insert(PICKER_INPUT_METADATA.to_owned(), input.to_owned());
    }
    proxy
        .set_one_with_persistence(
            identity.primary().clone(),
            OVERRIDE_RELATION,
            icon_target(glyph.to_owned()),
            metadata,
            true,
        )
        .await
        .map_err(|error| format!("set locus icon override relation: {error}"))?;
    for legacy_subject in identity.subjects.iter().skip(1) {
        clear_subject(&proxy, legacy_subject.clone()).await?;
    }
    Ok(())
}

async fn clear_override_async(workspace_id: String, workspace_name: String) -> Result<(), String> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    let proxy = locus::RelationsProxy::new(&connection)
        .await
        .map_err(|error| format!("connect locus proxy: {error}"))?;
    let identity = WorkspaceIconIdentity::new(workspace_id, Some(&workspace_name));
    for subject in identity.subjects {
        clear_subject(&proxy, subject).await?;
    }
    Ok(())
}

async fn clear_subject(
    proxy: &locus::RelationsProxy<'_>,
    subject: RelationEndpoint,
) -> Result<(), String> {
    proxy
        .clear(subject, OVERRIDE_RELATION)
        .await
        .map(|_| ())
        .map_err(|error| format!("clear locus icon override relation: {error}"))
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}
