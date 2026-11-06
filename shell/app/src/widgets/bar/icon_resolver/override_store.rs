mod identity;

use std::collections::HashMap;

use locus::{RelationEndpoint, RelationsProxy};
use shell_core::source::{self, Observable, rx::Observable as _};
use zbus::Connection;

use super::IconChoice;
use crate::widgets::bar::workspace::WorkspaceObject;
use identity::{WorkspaceIconIdentity, icon_choice_from_record, icon_target};

#[cfg(test)]
pub(in crate::widgets::bar) use identity::workspace_icon_subjects_for_test;

const WORKSPACE_ICON_OVERRIDE_RELATION: &str = "org.rsynapse.workspace.icon-override";
const PICKER_INPUT_METADATA: &str = "pick-icon-input";

pub(in crate::widgets::bar) fn workspace_icon_override_source(
    workspace: WorkspaceObject,
) -> Observable<Option<IconChoice>> {
    let identity = workspace
        .id()
        .combine_latest(workspace.name(), |id, name| {
            WorkspaceIconIdentity::new(id, name.as_deref())
        })
        .box_it();
    source::switch_map(identity, locus_workspace_icon_override)
        .distinct_until_changed()
        .box_it()
}

pub(in crate::widgets::bar) fn set_workspace_icon_override(
    workspace_id: String,
    workspace_name: String,
    icon: IconChoice,
    picker_input: String,
) {
    source::write::spawn("workspace icon override", async move {
        set_workspace_icon_override_async(workspace_id, workspace_name, icon, picker_input)
            .await
            .map_err(|error| error)
    });
}

pub(in crate::widgets::bar) fn clear_workspace_icon_override(
    workspace_id: String,
    workspace_name: String,
) {
    source::write::spawn("workspace icon clear", async move {
        clear_workspace_icon_override_async(workspace_id, workspace_name)
            .await
            .map_err(|error| error)
    });
}

fn locus_workspace_icon_override(
    identity: WorkspaceIconIdentity,
) -> Observable<Option<IconChoice>> {
    let subjects = identity.subjects;
    let filter_subjects = subjects.clone();
    source::watch_relation(subjects, WORKSPACE_ICON_OVERRIDE_RELATION)
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

async fn set_workspace_icon_override_async(
    workspace_id: String,
    workspace_name: String,
    icon: IconChoice,
    picker_input: String,
) -> Result<(), String> {
    let glyph = non_empty(icon.glyph).ok_or_else(|| "empty icon override".to_string())?;
    let connection = Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    let proxy = RelationsProxy::new(&connection)
        .await
        .map_err(|error| format!("connect locus proxy: {error}"))?;
    let identity = WorkspaceIconIdentity::new(workspace_id, Some(&workspace_name));
    let target = icon_target(glyph);
    let mut metadata = HashMap::new();
    if let Some(input) = non_empty(picker_input) {
        metadata.insert(PICKER_INPUT_METADATA.to_owned(), input);
    }
    proxy
        .set_one_with_persistence(
            identity.primary().clone(),
            WORKSPACE_ICON_OVERRIDE_RELATION,
            target,
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

async fn clear_workspace_icon_override_async(
    workspace_id: String,
    workspace_name: String,
) -> Result<(), String> {
    let connection = Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    let proxy = RelationsProxy::new(&connection)
        .await
        .map_err(|error| format!("connect locus proxy: {error}"))?;
    let identity = WorkspaceIconIdentity::new(workspace_id, Some(&workspace_name));
    for subject in identity.subjects {
        clear_subject(&proxy, subject).await?;
    }
    Ok(())
}

async fn clear_subject(
    proxy: &RelationsProxy<'_>,
    subject: RelationEndpoint,
) -> Result<(), String> {
    proxy
        .clear(subject, WORKSPACE_ICON_OVERRIDE_RELATION)
        .await
        .map(|_| ())
        .map_err(|error| format!("clear locus icon override relation: {error}"))
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}
