//! Shared Locus relation watch primitive.
//!
//! Subscribes to all four relation signals before reading the initial `List`
//! snapshot so associations changed during startup cannot be lost. The
//! snapshot read uses a separate connection from the signal watcher so bounded
//! signal queues cannot stall it. Stream termination and subscription failures
//! are emitted as source errors instead of ending silently.
use futures_util::StreamExt;
use locus::{RelationEndpoint, RelationRecord, RelationsProxy};

use crate::{Observable, rx::Observable as _};

/// Live records for `subjects` in one relation, refreshed on every matching
/// relation signal.
pub fn watch_relation(
    subjects: Vec<RelationEndpoint>,
    relation: &'static str,
) -> Observable<Vec<RelationRecord>> {
    let key = format!("{relation}|{subjects:?}");
    crate::shared_by_key("rsynapse.locus-relation", key, move || {
        let subjects = subjects.clone();
        crate::from_task(move |sender| {
            let subjects = subjects.clone();
            let error_sender = sender.clone();
            async move {
                if let Err(error) =
                    run_relation_watch(sender, subjects, relation, |records| records).await
                {
                    let _ = error_sender.send(Err(error)).await;
                }
            }
        })
        .distinct_until_changed()
        .box_it()
    })
}

/// Convenience wrapper for a single subject.
pub fn watch_relation_subject(
    subject: RelationEndpoint,
    relation: &'static str,
) -> Observable<Vec<RelationRecord>> {
    watch_relation(vec![subject], relation)
}

/// Runs the watch loop and maps record snapshots into `T`.
///
/// Exposed to crate tests that need to exercise ordering/backpressure without
/// subscribing through the Rx adapter.
pub(crate) async fn run_relation_watch<T, F>(
    sender: async_channel::Sender<Result<T, String>>,
    subjects: Vec<RelationEndpoint>,
    relation: &'static str,
    map: F,
) -> Result<(), String>
where
    T: Send + 'static,
    F: Fn(Vec<RelationRecord>) -> T + Send + 'static,
{
    let connection = zbus::Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    let proxy = RelationsProxy::new(&connection)
        .await
        .map_err(|error| format!("connect locus proxy: {error}"))?;
    // Snapshot replies must not share a connection with bounded signal queues
    // that can fill while the snapshot is in flight.
    let reads = zbus::Connection::session()
        .await
        .map_err(|error| format!("connect locus reads: {error}"))?;
    let reader = RelationsProxy::new(&reads)
        .await
        .map_err(|error| format!("connect locus reader: {error}"))?;

    // Subscribe before the initial List so changes made during startup are not
    // lost in a List/subscribe gap.
    let mut added = Box::pin(
        proxy
            .receive_relation_added()
            .await
            .map_err(|error| format!("subscribe locus RelationAdded: {error}"))?,
    );
    let mut updated = Box::pin(
        proxy
            .receive_relation_updated()
            .await
            .map_err(|error| format!("subscribe locus RelationUpdated: {error}"))?,
    );
    let mut removed = Box::pin(
        proxy
            .receive_relation_removed()
            .await
            .map_err(|error| format!("subscribe locus RelationRemoved: {error}"))?,
    );
    let mut cleared = Box::pin(
        proxy
            .receive_relation_cleared()
            .await
            .map_err(|error| format!("subscribe locus RelationCleared: {error}"))?,
    );

    emit(&sender, &reader, &subjects, relation, &map).await?;

    loop {
        let changed = tokio::select! {
            message = added.next() => {
                let Some(message) = message else {
                    return Err("locus RelationAdded stream ended".to_owned());
                };
                let args = message.args().map_err(|e| e.to_string())?;
                args.record.relation == relation && subjects.contains(&args.record.subject)
            }
            message = updated.next() => {
                let Some(message) = message else {
                    return Err("locus RelationUpdated stream ended".to_owned());
                };
                let args = message.args().map_err(|e| e.to_string())?;
                args.record.relation == relation && subjects.contains(&args.record.subject)
            }
            message = removed.next() => {
                let Some(message) = message else {
                    return Err("locus RelationRemoved stream ended".to_owned());
                };
                let args = message.args().map_err(|e| e.to_string())?;
                args.record.relation == relation && subjects.contains(&args.record.subject)
            }
            message = cleared.next() => {
                let Some(message) = message else {
                    return Err("locus RelationCleared stream ended".to_owned());
                };
                let args = message.args().map_err(|e| e.to_string())?;
                args.relation == relation && subjects.contains(&args.subject)
            }
        };
        if changed {
            emit(&sender, &reader, &subjects, relation, &map).await?;
        }
    }
}

async fn emit<T, F>(
    sender: &async_channel::Sender<Result<T, String>>,
    reader: &RelationsProxy<'_>,
    subjects: &[RelationEndpoint],
    relation: &str,
    map: &F,
) -> Result<(), String>
where
    F: Fn(Vec<RelationRecord>) -> T,
{
    let records = list_records(reader, subjects, relation).await?;
    sender
        .send(Ok(map(records)))
        .await
        .map_err(|_| "locus relation subscriber dropped".to_string())
}

async fn list_records(
    reader: &RelationsProxy<'_>,
    subjects: &[RelationEndpoint],
    relation: &str,
) -> Result<Vec<RelationRecord>, String> {
    match reader.list(relation).await {
        Ok(records) => Ok(records
            .into_iter()
            .filter(|record| subjects.contains(&record.subject))
            .collect()),
        Err(error) if is_locus_unavailable(&error) => Ok(Vec::new()),
        Err(error) => Err(format!("read locus relations {relation}: {error}")),
    }
}

pub(crate) fn is_locus_unavailable(error: &zbus::Error) -> bool {
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
