use std::collections::HashMap;

use futures_util::StreamExt;
use locus::{RelationEndpoint, RelationRecord, keys};
use shell_core::source::{self, Observable, rx::Observable as _};
use zbus::{Connection, Proxy};

use super::{ProjectDetails, non_empty};
use crate::widgets::bar::niri::NiriWorkspace;

const WORKSPACE_PROJECT_RELATION: &str = "org.rsynapse.workspace.project";

pub(super) fn project_details(workspace: NiriWorkspace) -> Observable<ProjectDetails> {
    source::switch_map(workspace.id().map(workspace_subject).box_it(), |subject| {
        locus_workspace_project(subject)
    })
    .distinct_until_changed()
    .box_it()
}

fn locus_workspace_project(subject: RelationEndpoint) -> Observable<ProjectDetails> {
    let key = format!("{subject:?}");
    source::shared_by_key("rsynapse.workspace-project", key, move || {
        let subject = subject.clone();
        source::from_task(move |sender| {
            let subject = subject.clone();
            async move {
                let Err(error) = run_locus_workspace_project(sender, subject.clone()).await else {
                    return;
                };
                eprintln!(
                    "[project-source] failed to watch locus project for {subject:?}: {error}"
                );
            }
        })
        .distinct_until_changed()
        .box_it()
    })
}

async fn run_locus_workspace_project(
    sender: async_channel::Sender<Result<ProjectDetails, String>>,
    subject: RelationEndpoint,
) -> Result<(), String> {
    let connection = Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    let proxy = locus_proxy(&connection)
        .await
        .map_err(|error| format!("connect locus proxy: {error}"))?;

    send_project(&sender, &proxy, &subject).await?;

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
                    send_project(&sender, &proxy, &subject).await?;
                }
            }
            message = updated.next() => {
                let Some(message) = message else { return Ok(()); };
                if relation_record_matches(&message, &subject)? {
                    send_project(&sender, &proxy, &subject).await?;
                }
            }
            message = removed.next() => {
                let Some(message) = message else { return Ok(()); };
                if relation_record_matches(&message, &subject)? {
                    send_project(&sender, &proxy, &subject).await?;
                }
            }
            message = cleared.next() => {
                let Some(message) = message else { return Ok(()); };
                if clear_matches(&message, &subject)? {
                    send_project(&sender, &proxy, &subject).await?;
                }
            }
        }
    }
}

async fn send_project(
    sender: &async_channel::Sender<Result<ProjectDetails, String>>,
    proxy: &Proxy<'_>,
    subject: &RelationEndpoint,
) -> Result<(), String> {
    let records = match proxy
        .call::<_, _, Vec<RelationRecord>>("List", &(WORKSPACE_PROJECT_RELATION,))
        .await
    {
        Ok(records) => records,
        Err(error) if is_locus_unavailable(&error) => Vec::new(),
        Err(error) => return Err(format!("read locus project relations: {error}")),
    };
    let project = records
        .into_iter()
        .find(|record| record.subject == *subject)
        .map(ProjectDetails::from)
        .unwrap_or_default();
    sender
        .send(Ok(project))
        .await
        .map_err(|_| "project relation subscriber dropped".to_string())
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
    Ok(record.subject == *subject && record.relation == WORKSPACE_PROJECT_RELATION)
}

fn clear_matches(message: &zbus::Message, subject: &RelationEndpoint) -> Result<bool, String> {
    let (cleared_subject, cleared_relation, _count) = message
        .body()
        .deserialize::<(RelationEndpoint, String, u32)>()
        .map_err(|error| format!("decode locus clear signal: {error}"))?;
    Ok(cleared_subject == *subject && cleared_relation == WORKSPACE_PROJECT_RELATION)
}

impl From<RelationRecord> for ProjectDetails {
    fn from(record: RelationRecord) -> Self {
        Self {
            has_project: true,
            name: metadata_value(&record.metadata, &["display-main"]),
            branch: metadata_value(&record.metadata, &["display-secondary"]),
            icon: metadata_value(&record.metadata, &["display-icon", "icon"]),
        }
    }
}

fn metadata_value(metadata: &HashMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| non_empty(metadata.get(*key).cloned()))
}

fn workspace_subject(id: u64) -> RelationEndpoint {
    RelationEndpoint::stable_key(keys::NIRI_WORKSPACE_ID, id.to_string())
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
