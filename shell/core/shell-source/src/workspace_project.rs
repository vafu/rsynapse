//! Reactive workspace -> project association (locus) resolved through projd.
use std::collections::HashMap;

use futures_util::StreamExt;
use locus::{RelationEndpoint, RelationRecord, keys};
use proj_model::ProjectInfo;
use zbus::{Connection, Proxy};

use crate::workspace::WorkspaceObject;
use crate::{Observable, rx::Observable as _};

#[cfg(test)]
mod backpressure;
mod projd;

const WORKSPACE_PROJECT_RELATION: &str = "org.rsynapse.workspace.project";

/// Resolved workspace association. `has_project` reflects locus presence;
/// `project`/`branch` fill in once projd data is available.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkspaceProject {
    pub has_project: bool,
    pub path: Option<String>,
    pub project_id: Option<String>,
    pub project: Option<ProjectInfo>,
    pub branch: Option<String>,
}

/// Live project association for one workspace.
pub fn workspace_project(workspace: WorkspaceObject) -> Observable<WorkspaceProject> {
    projd::resolve(
        crate::switch_map(workspace.id().map(workspace_subject).box_it(), |subject| {
            locus_workspace_project(subject)
        })
        .box_it(),
    )
}

/// Live project association for the focused (or named output's) workspace.
pub fn focused_workspace_project(output: Option<String>) -> Observable<WorkspaceProject> {
    crate::switch_map(
        crate::workspace::current_workspace(output),
        |workspace| match workspace {
            Some(workspace) => workspace_project(workspace),
            None => crate::once(WorkspaceProject::default()),
        },
    )
}

/// Live resolved projd project for the focused (or named output's) workspace.
pub fn focused_project(output: Option<String>) -> Observable<Option<ProjectInfo>> {
    focused_workspace_project(output)
        .map(|project| project.project)
        .distinct_until_changed()
        .box_it()
}

/// Live project ID for the focused (or named output's) workspace.
pub fn focused_project_id(output: Option<String>) -> Observable<Option<String>> {
    focused_workspace_project(output)
        .map(|project| project.project_id)
        .distinct_until_changed()
        .box_it()
}

fn locus_workspace_project(subject: RelationEndpoint) -> Observable<WorkspaceProject> {
    let key = format!("{subject:?}");
    crate::shared_by_key("rsynapse.workspace-project", key, move || {
        let subject = subject.clone();
        crate::from_task(move |sender| {
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
    sender: async_channel::Sender<Result<WorkspaceProject, String>>,
    subject: RelationEndpoint,
) -> Result<(), String> {
    let connection = Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    let proxy = locus_proxy(&connection)
        .await
        .map_err(|error| format!("connect locus proxy: {error}"))?;
    // Do not await List replies on the connection whose bounded signal queues
    // can fill during hotplug/association replay. Otherwise the display watcher
    // can stall even after Steward successfully commits the assignment.
    let reads = Connection::session()
        .await
        .map_err(|error| format!("connect locus reads: {error}"))?;
    let reader = locus_proxy(&reads)
        .await
        .map_err(|error| format!("connect locus reader: {error}"))?;

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

    send_project(&sender, &reader, &subject).await?;

    loop {
        tokio::select! {
            message = added.next() => {
                let Some(message) = message else { return Ok(()); };
                if relation_record_matches(&message, &subject)? {
                    send_project(&sender, &reader, &subject).await?;
                }
            }
            message = updated.next() => {
                let Some(message) = message else { return Ok(()); };
                if relation_record_matches(&message, &subject)? {
                    send_project(&sender, &reader, &subject).await?;
                }
            }
            message = removed.next() => {
                let Some(message) = message else { return Ok(()); };
                if relation_record_matches(&message, &subject)? {
                    send_project(&sender, &reader, &subject).await?;
                }
            }
            message = cleared.next() => {
                let Some(message) = message else { return Ok(()); };
                if clear_matches(&message, &subject)? {
                    send_project(&sender, &reader, &subject).await?;
                }
            }
        }
    }
}

async fn send_project(
    sender: &async_channel::Sender<Result<WorkspaceProject, String>>,
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
        .map(WorkspaceProject::from)
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

impl From<RelationRecord> for WorkspaceProject {
    fn from(record: RelationRecord) -> Self {
        let path = project_path_from_endpoint(&record.target)
            .or_else(|| metadata_value(&record.metadata, &["path"]));
        Self {
            has_project: true,
            path,
            project_id: metadata_value(&record.metadata, &["project-id"]),
            ..Self::default()
        }
    }
}

fn project_path_from_endpoint(endpoint: &RelationEndpoint) -> Option<String> {
    match endpoint {
        RelationEndpoint::StableKey { kind, id } if kind == keys::PROJECT_PATH => {
            non_empty(Some(id.clone()))
        }
        _ => None,
    }
}

fn metadata_value(metadata: &HashMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| non_empty(metadata.get(*key).cloned()))
}

fn workspace_subject(id: String) -> RelationEndpoint {
    RelationEndpoint::stable_key(keys::WORKSPACE_ID, id)
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    })
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
