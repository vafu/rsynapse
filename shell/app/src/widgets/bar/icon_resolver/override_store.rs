use std::collections::HashMap;

use futures_util::StreamExt;
use locus::{RelationEndpoint, RelationRecord, keys};
use shell_core::source::{self, Observable, rx::Observable as _};
use zbus::{Connection, Proxy};

use super::IconChoice;
use crate::widgets::bar::niri::NiriWorkspace;

const WORKSPACE_ICON_OVERRIDE_RELATION: &str = "org.rsynapse.workspace.icon-override";
const ICON_KEY_KIND: &str = "org.rsynapse.icon.key";
const LEGACY_MATERIAL_ICON_KIND: &str = "org.rsynapse.material-icon.name";
const ICON_GLYPH_METADATA: &str = "icon-glyph";
const PICKER_INPUT_METADATA: &str = "pick-icon-input";

pub(in crate::widgets::bar) fn workspace_icon_override_source(
    workspace: NiriWorkspace,
) -> Observable<Option<IconChoice>> {
    source::switch_map(workspace.id().map(workspace_subject).box_it(), |subject| {
        locus_workspace_icon_override(subject)
    })
    .distinct_until_changed()
    .box_it()
}

pub(in crate::widgets::bar) fn set_workspace_icon_override(
    workspace_id: u64,
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
            icon,
            picker_input,
        )) {
            eprintln!("[workspace-icon-override] failed to set override: {error}");
        }
    });
}

pub(in crate::widgets::bar) fn clear_workspace_icon_override(workspace_id: u64) {
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
        if let Err(error) = runtime.block_on(clear_workspace_icon_override_async(workspace_id)) {
            eprintln!("[workspace-icon-override] failed to clear override: {error}");
        }
    });
}

fn locus_workspace_icon_override(subject: RelationEndpoint) -> Observable<Option<IconChoice>> {
    let key = format!("{subject:?}");
    source::shared_by_key("rsynapse.workspace-icon-override", key, move || {
        let subject = subject.clone();
        source::from_task(move |sender| {
            let subject = subject.clone();
            async move {
                let Err(error) = run_locus_workspace_icon_override(sender, subject.clone()).await
                else {
                    return;
                };
                eprintln!(
                    "[workspace-icon-override] failed to watch locus override for {subject:?}: {error}"
                );
            }
        })
        .distinct_until_changed()
        .box_it()
    })
}

async fn run_locus_workspace_icon_override(
    sender: async_channel::Sender<Result<Option<IconChoice>, String>>,
    subject: RelationEndpoint,
) -> Result<(), String> {
    let connection = Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    let proxy = locus_proxy(&connection)
        .await
        .map_err(|error| format!("connect locus proxy: {error}"))?;

    send_override(&sender, &proxy, &subject).await?;

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
                if relation_record_matches(&message, &subject)? {
                    send_override(&sender, &proxy, &subject).await?;
                }
            }
            message = updated.next() => {
                let Some(message) = message else { return Ok(()); };
                if relation_record_matches(&message, &subject)? {
                    send_override(&sender, &proxy, &subject).await?;
                }
            }
            message = removed.next() => {
                let Some(message) = message else { return Ok(()); };
                if relation_record_matches(&message, &subject)? {
                    send_override(&sender, &proxy, &subject).await?;
                }
            }
            message = cleared.next() => {
                let Some(message) = message else { return Ok(()); };
                if clear_matches(&message, &subject)? {
                    send_override(&sender, &proxy, &subject).await?;
                }
            }
        }
    }
}

async fn send_override(
    sender: &async_channel::Sender<Result<Option<IconChoice>, String>>,
    proxy: &Proxy<'_>,
    subject: &RelationEndpoint,
) -> Result<(), String> {
    let records = match proxy
        .call::<_, _, Vec<RelationRecord>>("List", &(WORKSPACE_ICON_OVERRIDE_RELATION,))
        .await
    {
        Ok(records) => records,
        Err(error) if is_locus_unavailable(&error) => Vec::new(),
        Err(error) => return Err(format!("read locus icon override relations: {error}")),
    };
    let icon = records
        .into_iter()
        .find(|record| record.subject == *subject)
        .and_then(|record| icon_choice_from_record(&record));
    sender
        .send(Ok(icon))
        .await
        .map_err(|_| "workspace icon override subscriber dropped".to_string())
}

async fn set_workspace_icon_override_async(
    workspace_id: u64,
    icon: IconChoice,
    picker_input: String,
) -> Result<(), String> {
    let icon_name = non_empty(icon.icon).ok_or_else(|| "empty icon override".to_string())?;
    let connection = Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    let proxy = locus_proxy(&connection)
        .await
        .map_err(|error| format!("connect locus proxy: {error}"))?;
    let subject = workspace_subject(workspace_id);
    let target = RelationEndpoint::stable_key(ICON_KEY_KIND, icon_name);
    let mut metadata = HashMap::new();
    if let Some(glyph) = icon.glyph.and_then(non_empty) {
        metadata.insert(ICON_GLYPH_METADATA.to_owned(), glyph);
    }
    if let Some(input) = non_empty(picker_input) {
        metadata.insert(PICKER_INPUT_METADATA.to_owned(), input);
    }
    proxy
        .call::<_, _, RelationRecord>(
            "SetOne",
            &(subject, WORKSPACE_ICON_OVERRIDE_RELATION, target, metadata),
        )
        .await
        .map(|_| ())
        .map_err(|error| format!("set locus icon override relation: {error}"))
}

async fn clear_workspace_icon_override_async(workspace_id: u64) -> Result<(), String> {
    let connection = Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    let proxy = locus_proxy(&connection)
        .await
        .map_err(|error| format!("connect locus proxy: {error}"))?;
    let subject = workspace_subject(workspace_id);
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
    subject: &RelationEndpoint,
) -> Result<bool, String> {
    let record = message
        .body()
        .deserialize::<RelationRecord>()
        .map_err(|error| format!("decode locus relation signal: {error}"))?;
    Ok(record.subject == *subject && record.relation == WORKSPACE_ICON_OVERRIDE_RELATION)
}

fn clear_matches(message: &zbus::Message, subject: &RelationEndpoint) -> Result<bool, String> {
    let (cleared_subject, cleared_relation, _count) = message
        .body()
        .deserialize::<(RelationEndpoint, String, u32)>()
        .map_err(|error| format!("decode locus clear signal: {error}"))?;
    Ok(cleared_subject == *subject && cleared_relation == WORKSPACE_ICON_OVERRIDE_RELATION)
}

fn icon_choice_from_record(record: &RelationRecord) -> Option<IconChoice> {
    let icon = icon_name_from_endpoint(&record.target)?;
    let glyph = metadata_value(
        &record.metadata,
        &[ICON_GLYPH_METADATA, "display-icon-glyph", "glyph"],
    );
    IconChoice::new(icon, glyph)
}

fn icon_name_from_endpoint(endpoint: &RelationEndpoint) -> Option<String> {
    match endpoint {
        RelationEndpoint::StableKey { kind, id }
            if kind == ICON_KEY_KIND || kind == LEGACY_MATERIAL_ICON_KIND =>
        {
            non_empty(id.clone())
        }
        _ => None,
    }
}

fn workspace_subject(id: u64) -> RelationEndpoint {
    RelationEndpoint::stable_key(keys::NIRI_WORKSPACE_ID, id.to_string())
}

fn metadata_value(metadata: &HashMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| metadata.get(*key).cloned().and_then(non_empty))
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
