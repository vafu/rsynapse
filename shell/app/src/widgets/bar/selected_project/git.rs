use futures_util::StreamExt;
use locus::{RelationEndpoint, RelationRecord};
use shell_core::source::{self, Observable, rx::Observable as _};
use zbus::{Connection, Proxy};

#[cfg(test)]
mod test;

const PROJECT_GIT_STATUS_RELATION: &str = "org.rsynapse.project.git-status";

/// Pure-style working-tree status for one project directory.
///
/// Counts mirror `git status --porcelain=v2` entry kinds; `merging` and
/// `rebasing` come from the git dir sentinel files, `stashes` from
/// `git stash list`. The default (all zero/false) also represents
/// non-repositories and read failures, keeping the widget invisible.
///
/// Values arrive from the steward git poller through locus relations: this
/// module subscribes but never shells out, so slow porcelain scans on large
/// worktrees cannot stall UI runtime workers.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct GitStatus {
    pub(super) staged: usize,
    pub(super) unstaged: usize,
    pub(super) untracked: usize,
    pub(super) ahead: u32,
    pub(super) behind: u32,
    pub(super) merging: bool,
    pub(super) rebasing: bool,
    pub(super) stashes: usize,
}

pub(super) fn git_status(path: String) -> Observable<GitStatus> {
    source::shared_by_key("rsynapse.git-status", path.clone(), move || {
        project_git_status(path.clone())
            .distinct_until_changed()
            .box_it()
    })
}

impl GitStatus {
    pub(super) fn has_changes(&self) -> bool {
        self.staged > 0
            || self.unstaged > 0
            || self.untracked > 0
            || self.ahead > 0
            || self.behind > 0
            || self.merging
            || self.rebasing
    }
}

fn project_git_status(path: String) -> Observable<GitStatus> {
    let subject = RelationEndpoint::stable_key(locus::keys::PROJECT_PATH, path);
    source::from_task(move |sender| {
        let subject = subject.clone();
        async move {
            let Err(error) = run_git_status_watch(sender, subject.clone()).await else {
                return;
            };
            eprintln!("[git-status] failed to watch locus git status for {subject:?}: {error}");
        }
    })
}

async fn run_git_status_watch(
    sender: async_channel::Sender<Result<GitStatus, String>>,
    subject: RelationEndpoint,
) -> Result<(), String> {
    let connection = Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    let proxy = locus_proxy(&connection)
        .await
        .map_err(|error| format!("connect locus proxy: {error}"))?;

    send_git_status(&sender, &proxy, &subject).await?;

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
                    send_git_status(&sender, &proxy, &subject).await?;
                }
            }
            message = updated.next() => {
                let Some(message) = message else { return Ok(()); };
                if relation_record_matches(&message, &subject)? {
                    send_git_status(&sender, &proxy, &subject).await?;
                }
            }
            message = removed.next() => {
                let Some(message) = message else { return Ok(()); };
                if relation_record_matches(&message, &subject)? {
                    send_git_status(&sender, &proxy, &subject).await?;
                }
            }
            message = cleared.next() => {
                let Some(message) = message else { return Ok(()); };
                if clear_matches(&message, &subject)? {
                    send_git_status(&sender, &proxy, &subject).await?;
                }
            }
        }
    }
}

async fn send_git_status(
    sender: &async_channel::Sender<Result<GitStatus, String>>,
    proxy: &Proxy<'_>,
    subject: &RelationEndpoint,
) -> Result<(), String> {
    let status = match proxy
        .call::<_, _, Vec<RelationRecord>>("List", &(PROJECT_GIT_STATUS_RELATION,))
        .await
    {
        Ok(records) => records
            .into_iter()
            .find(|record| record.subject == *subject)
            .map(|record| GitStatus::from_metadata(&record.metadata))
            .unwrap_or_default(),
        Err(error) if is_locus_unavailable(&error) => GitStatus::default(),
        Err(error) => return Err(format!("read locus git status: {error}")),
    };
    sender
        .send(Ok(status))
        .await
        .map_err(|_| "git status subscriber dropped".to_string())
}

impl GitStatus {
    fn from_metadata(metadata: &std::collections::HashMap<String, String>) -> Self {
        Self {
            staged: count(metadata, "staged"),
            unstaged: count(metadata, "unstaged"),
            untracked: count(metadata, "untracked"),
            ahead: count(metadata, "ahead"),
            behind: count(metadata, "behind"),
            merging: metadata.contains_key("merging"),
            rebasing: metadata.contains_key("rebasing"),
            stashes: count(metadata, "stashes"),
        }
    }
}

fn count<T>(metadata: &std::collections::HashMap<String, String>, key: &str) -> T
where
    T: Default + std::str::FromStr,
{
    metadata
        .get(key)
        .and_then(|value| value.parse().ok())
        .unwrap_or_default()
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
    Ok(record.subject == *subject && record.relation == PROJECT_GIT_STATUS_RELATION)
}

fn clear_matches(message: &zbus::Message, subject: &RelationEndpoint) -> Result<bool, String> {
    let (cleared_subject, cleared_relation, _count) = message
        .body()
        .deserialize::<(RelationEndpoint, String, u32)>()
        .map_err(|error| format!("decode locus clear signal: {error}"))?;
    Ok(cleared_subject == *subject && cleared_relation == PROJECT_GIT_STATUS_RELATION)
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
