use std::{collections::HashMap, path::Path};

use futures_util::StreamExt;
use locus::{RelationEndpoint, RelationRecord, keys};
use shell_core::source::{self, Observable, rx::Observable as _};
use zbus::{Connection, Proxy};

use super::niri::NiriWorkspace;

const WORKSPACE_PROJECT_RELATION: &str = "org.rsynapse.workspace.project";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::widgets::bar) struct ProjectDetails {
    pub(in crate::widgets::bar) has_project: bool,
    pub(in crate::widgets::bar) display_main: Option<String>,
    pub(in crate::widgets::bar) display_secondary: Option<String>,
    pub(in crate::widgets::bar) icon: Option<String>,
    pub(in crate::widgets::bar) icon_glyph: Option<String>,
    pub(in crate::widgets::bar) branch: Option<String>,
    pub(in crate::widgets::bar) cwd_label: Option<String>,
}

pub(in crate::widgets::bar) fn project_details(
    workspace: NiriWorkspace,
) -> Observable<ProjectDetails> {
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
        let path = project_path_from_endpoint(&record.target)
            .or_else(|| metadata_value(&record.metadata, &["path"]));
        let cwd_path = metadata_value(&record.metadata, &["cwd-path"]);
        let relative_cwd = metadata_value(&record.metadata, &["relative-cwd", "cwd"])
            .or_else(|| relative_cwd_from_paths(path.as_deref(), cwd_path.as_deref()));
        let cwd_label = cwd_label(
            relative_cwd.as_deref(),
            cwd_path.as_deref(),
            path.as_deref(),
        );

        Self {
            has_project: true,
            display_main: metadata_value(&record.metadata, &["display-main"]),
            display_secondary: metadata_value(&record.metadata, &["display-secondary"]),
            icon: metadata_value(&record.metadata, &["display-icon", "icon"]),
            icon_glyph: metadata_value(
                &record.metadata,
                &["display-icon-glyph", "icon-glyph", "glyph"],
            ),
            branch: metadata_value(&record.metadata, &["branch"]),
            cwd_label,
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

fn relative_cwd_from_paths(root: Option<&str>, cwd: Option<&str>) -> Option<String> {
    let root = Path::new(non_empty_str(root?)?);
    let cwd = Path::new(non_empty_str(cwd?)?);
    let relative = cwd.strip_prefix(root).ok()?.to_string_lossy().into_owned();
    non_root_relative(relative)
}

fn cwd_label(
    relative_cwd: Option<&str>,
    cwd_path: Option<&str>,
    project_path: Option<&str>,
) -> Option<String> {
    relative_cwd
        .and_then(non_empty_str)
        .filter(|value| *value != ".")
        .map(str::to_owned)
        .or_else(|| path_basename(cwd_path))
        .or_else(|| path_basename(project_path))
}

fn path_basename(path: Option<&str>) -> Option<String> {
    let path = Path::new(non_empty_str(path?)?);
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(non_empty_str)
        .map(str::to_owned)
}

fn metadata_value(metadata: &HashMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| non_empty(metadata.get(*key).cloned()))
}

fn workspace_subject(id: u64) -> RelationEndpoint {
    RelationEndpoint::stable_key(keys::NIRI_WORKSPACE_ID, id.to_string())
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    })
}

fn non_empty_str(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn non_root_relative(value: String) -> Option<String> {
    non_empty(Some(value)).filter(|value| value != ".")
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
