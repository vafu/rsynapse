use futures_util::StreamExt;
use locus::{RelationEndpoint, RelationRecord};
use shell_core::source::{self, Observable, rx::Observable as _};
use zbus::{Connection, Proxy};

const WORKSPACE_NAME_RELATION: &str = "org.rsynapse.workspace.name";
const WORKSPACE_NAME_KIND: &str = "org.rsynapse.workspace.name";

/// Display name for one workspace id, from the steward-owned name relation.
/// Emits `None` while unknown; the view falls back to project metadata.
pub(super) fn workspace_display_name(workspace_id: Option<u64>) -> Observable<Option<String>> {
    let Some(id) = workspace_id else {
        return source::once(None);
    };
    let subject = RelationEndpoint::stable_key(locus::keys::NIRI_WORKSPACE_ID, id.to_string());
    source::shared_by_key("rsynapse.workspace-name", id.to_string(), move || {
        watch_workspace_name(subject.clone())
            .distinct_until_changed()
            .box_it()
    })
}

fn watch_workspace_name(subject: RelationEndpoint) -> Observable<Option<String>> {
    source::from_task(move |sender| {
        let subject = subject.clone();
        async move {
            let Err(error) = run_name_watch(sender, subject.clone()).await else {
                return;
            };
            eprintln!("[workspace-name] failed to watch locus names for {subject:?}: {error}");
        }
    })
}

async fn run_name_watch(
    sender: async_channel::Sender<Result<Option<String>, String>>,
    subject: RelationEndpoint,
) -> Result<(), String> {
    let connection = Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    let proxy = locus_proxy(&connection)
        .await
        .map_err(|error| format!("connect locus proxy: {error}"))?;

    send_name(&sender, &proxy, &subject).await?;

    let mut added = Box::pin(
        proxy
            .receive_signal("RelationAdded")
            .await
            .map_err(to_string)?,
    );
    let mut updated = Box::pin(
        proxy
            .receive_signal("RelationUpdated")
            .await
            .map_err(to_string)?,
    );
    let mut removed = Box::pin(
        proxy
            .receive_signal("RelationRemoved")
            .await
            .map_err(to_string)?,
    );
    let mut cleared = Box::pin(
        proxy
            .receive_signal("RelationCleared")
            .await
            .map_err(to_string)?,
    );

    loop {
        tokio::select! {
            message = added.next() => {
                let Some(message) = message else { return Ok(()); };
                if relation_record_matches(&message, &subject)? {
                    send_name(&sender, &proxy, &subject).await?;
                }
            }
            message = updated.next() => {
                let Some(message) = message else { return Ok(()); };
                if relation_record_matches(&message, &subject)? {
                    send_name(&sender, &proxy, &subject).await?;
                }
            }
            message = removed.next() => {
                let Some(message) = message else { return Ok(()); };
                if relation_record_matches(&message, &subject)? {
                    send_name(&sender, &proxy, &subject).await?;
                }
            }
            message = cleared.next() => {
                let Some(message) = message else { return Ok(()); };
                if clear_matches(&message, &subject)? {
                    send_name(&sender, &proxy, &subject).await?;
                }
            }
        }
    }
}

async fn send_name(
    sender: &async_channel::Sender<Result<Option<String>, String>>,
    proxy: &Proxy<'_>,
    subject: &RelationEndpoint,
) -> Result<(), String> {
    let name = match proxy
        .call::<_, _, Vec<RelationRecord>>("List", &(WORKSPACE_NAME_RELATION,))
        .await
    {
        Ok(records) => records
            .into_iter()
            .find(|record| record.subject == *subject)
            .and_then(|record| name_of(&record)),
        Err(error) if is_locus_unavailable(&error) => None,
        Err(error) => return Err(format!("read locus workspace names: {error}")),
    };
    sender
        .send(Ok(name))
        .await
        .map_err(|_| "workspace name subscriber dropped".to_string())
}

fn name_of(record: &RelationRecord) -> Option<String> {
    match &record.target {
        RelationEndpoint::StableKey { kind, id } if kind == WORKSPACE_NAME_KIND => {
            let name = id.trim().to_owned();
            (!name.is_empty()).then_some(name)
        }
        _ => None,
    }
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
    Ok(record.subject == *subject && record.relation == WORKSPACE_NAME_RELATION)
}

fn clear_matches(message: &zbus::Message, subject: &RelationEndpoint) -> Result<bool, String> {
    let (cleared_subject, cleared_relation, _count) = message
        .body()
        .deserialize::<(RelationEndpoint, String, u32)>()
        .map_err(|error| format!("decode locus clear signal: {error}"))?;
    Ok(cleared_subject == *subject && cleared_relation == WORKSPACE_NAME_RELATION)
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
