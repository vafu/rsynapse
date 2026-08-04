mod identity;

use std::collections::HashMap;

use futures_util::StreamExt;
use locus::{RelationEndpoint, RelationRecord};
use shell_core::source::{self, Observable, rx::Observable as _};
use zbus::{Connection, Proxy};

use super::IconChoice;
use crate::widgets::bar::niri::NiriWorkspace;
use identity::{WorkspaceIconIdentity, icon_choice_from_record, icon_target};

#[cfg(test)]
pub(in crate::widgets::bar) use identity::workspace_icon_subjects_for_test;

const WORKSPACE_ICON_OVERRIDE_RELATION: &str = "org.rsynapse.workspace.icon-override";
const PICKER_INPUT_METADATA: &str = "pick-icon-input";

pub(in crate::widgets::bar) fn workspace_icon_override_source(
    workspace: NiriWorkspace,
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
    workspace_id: u64,
    workspace_name: String,
    icon: IconChoice,
    picker_input: String,
) {
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                eprintln!("[workspace-icon-override] failed to start runtime: {error}");
                return;
            }
        };
        if let Err(error) = runtime.block_on(set_workspace_icon_override_async(
            workspace_id,
            workspace_name,
            icon,
            picker_input,
        )) {
            eprintln!("[workspace-icon-override] failed to set override: {error}");
        }
    });
}

pub(in crate::widgets::bar) fn clear_workspace_icon_override(
    workspace_id: u64,
    workspace_name: String,
) {
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                eprintln!("[workspace-icon-override] failed to start runtime: {error}");
                return;
            }
        };
        if let Err(error) = runtime.block_on(clear_workspace_icon_override_async(
            workspace_id,
            workspace_name,
        )) {
            eprintln!("[workspace-icon-override] failed to clear override: {error}");
        }
    });
}

fn locus_workspace_icon_override(
    identity: WorkspaceIconIdentity,
) -> Observable<Option<IconChoice>> {
    let key = format!("{:?}", identity.subjects);
    source::shared_by_key("rsynapse.workspace-icon-override", key, move || {
        let identity = identity.clone();
        source::from_task(move |sender| {
            let identity = identity.clone();
            async move {
                let Err(error) = run_locus_workspace_icon_override(sender, identity.clone()).await
                else {
                    return;
                };
                eprintln!(
                    "[workspace-icon-override] failed to watch locus override for {:?}: {error}",
                    identity.subjects
                );
            }
        })
        .distinct_until_changed()
        .box_it()
    })
}

async fn run_locus_workspace_icon_override(
    sender: async_channel::Sender<Result<Option<IconChoice>, String>>,
    identity: WorkspaceIconIdentity,
) -> Result<(), String> {
    let connection = Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    let proxy = locus_proxy(&connection)
        .await
        .map_err(|error| format!("connect locus proxy: {error}"))?;

    send_override(&sender, &proxy, &identity.subjects).await?;

    macro_rules! signal {
        ($name:literal) => {
            Box::pin(proxy.receive_signal($name).await.map_err(to_string)?)
        };
    }
    let mut added = signal!("RelationAdded");
    let mut updated = signal!("RelationUpdated");
    let mut removed = signal!("RelationRemoved");
    let mut cleared = signal!("RelationCleared");

    loop {
        tokio::select! {
            message = added.next() => {
                let Some(message) = message else { return Ok(()); };
                if relation_record_matches(&message, &identity.subjects)? {
                    send_override(&sender, &proxy, &identity.subjects).await?;
                }
            }
            message = updated.next() => {
                let Some(message) = message else { return Ok(()); };
                if relation_record_matches(&message, &identity.subjects)? {
                    send_override(&sender, &proxy, &identity.subjects).await?;
                }
            }
            message = removed.next() => {
                let Some(message) = message else { return Ok(()); };
                if relation_record_matches(&message, &identity.subjects)? {
                    send_override(&sender, &proxy, &identity.subjects).await?;
                }
            }
            message = cleared.next() => {
                let Some(message) = message else { return Ok(()); };
                if clear_matches(&message, &identity.subjects)? {
                    send_override(&sender, &proxy, &identity.subjects).await?;
                }
            }
        }
    }
}

async fn send_override(
    sender: &async_channel::Sender<Result<Option<IconChoice>, String>>,
    proxy: &Proxy<'_>,
    subjects: &[RelationEndpoint],
) -> Result<(), String> {
    let records = match proxy
        .call::<_, _, Vec<RelationRecord>>("List", &(WORKSPACE_ICON_OVERRIDE_RELATION,))
        .await
    {
        Ok(records) => records,
        Err(error) if is_locus_unavailable(&error) => Vec::new(),
        Err(error) => return Err(format!("read locus icon override relations: {error}")),
    };
    let icon = subjects.iter().find_map(|subject| {
        records
            .iter()
            .find(|record| record.subject == *subject)
            .and_then(icon_choice_from_record)
    });
    sender
        .send(Ok(icon))
        .await
        .map_err(|_| "workspace icon override subscriber dropped".to_string())
}

async fn set_workspace_icon_override_async(
    workspace_id: u64,
    workspace_name: String,
    icon: IconChoice,
    picker_input: String,
) -> Result<(), String> {
    let glyph = non_empty(icon.glyph).ok_or_else(|| "empty icon override".to_string())?;
    let connection = Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    let proxy = locus_proxy(&connection)
        .await
        .map_err(|error| format!("connect locus proxy: {error}"))?;
    let identity = WorkspaceIconIdentity::new(workspace_id, Some(&workspace_name));
    let target = icon_target(glyph);
    let mut metadata = HashMap::new();
    if let Some(input) = non_empty(picker_input) {
        metadata.insert(PICKER_INPUT_METADATA.to_owned(), input);
    }
    proxy
        .call::<_, _, RelationRecord>(
            "SetOne",
            &(
                identity.primary().clone(),
                WORKSPACE_ICON_OVERRIDE_RELATION,
                target,
                metadata,
            ),
        )
        .await
        .map_err(|error| format!("set locus icon override relation: {error}"))?;
    for legacy_subject in identity.subjects.iter().skip(1) {
        clear_subject(&proxy, legacy_subject.clone()).await?;
    }
    Ok(())
}

async fn clear_workspace_icon_override_async(
    workspace_id: u64,
    workspace_name: String,
) -> Result<(), String> {
    let connection = Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    let proxy = locus_proxy(&connection)
        .await
        .map_err(|error| format!("connect locus proxy: {error}"))?;
    let identity = WorkspaceIconIdentity::new(workspace_id, Some(&workspace_name));
    for subject in identity.subjects {
        clear_subject(&proxy, subject).await?;
    }
    Ok(())
}

async fn clear_subject(proxy: &Proxy<'_>, subject: RelationEndpoint) -> Result<(), String> {
    proxy
        .call::<_, _, u32>("Clear", &(subject, WORKSPACE_ICON_OVERRIDE_RELATION))
        .await
        .map(|_| ())
        .map_err(|error| format!("clear locus icon override relation: {error}"))
}

async fn locus_proxy(connection: &Connection) -> zbus::Result<Proxy<'_>> {
    Proxy::new(
        connection,
        locus::BUS_NAME,
        locus::OBJECT_PATH,
        locus::RELATIONS_INTERFACE,
    )
    .await
}

fn relation_record_matches(
    message: &zbus::Message,
    subjects: &[RelationEndpoint],
) -> Result<bool, String> {
    let record = message
        .body()
        .deserialize::<RelationRecord>()
        .map_err(|error| format!("decode locus relation signal: {error}"))?;
    Ok(subjects.contains(&record.subject) && record.relation == WORKSPACE_ICON_OVERRIDE_RELATION)
}

fn clear_matches(message: &zbus::Message, subjects: &[RelationEndpoint]) -> Result<bool, String> {
    let (cleared_subject, cleared_relation, _count) = message
        .body()
        .deserialize::<(RelationEndpoint, String, u32)>()
        .map_err(|error| format!("decode locus clear signal: {error}"))?;
    Ok(subjects.contains(&cleared_subject) && cleared_relation == WORKSPACE_ICON_OVERRIDE_RELATION)
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn to_string(error: zbus::Error) -> String {
    error.to_string()
}

fn is_locus_unavailable(error: &zbus::Error) -> bool {
    match error {
        zbus::Error::MethodError(name, _, _) => {
            name.as_str() == "org.freedesktop.DBus.Error.ServiceUnknown"
        }
        zbus::Error::FDO(error) => {
            matches!(error.as_ref(), zbus::fdo::Error::ServiceUnknown(_))
        }
        _ => false,
    }
}
