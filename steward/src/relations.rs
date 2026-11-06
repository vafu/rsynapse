use std::collections::HashMap;

use futures_util::StreamExt;
use locus::{RelationEndpoint, RelationRecord, RelationsProxy};
use shell_source::{Observable, from_task, rx::Observable as _};
use zbus::Connection;

/// Workspace -> project link (owned by whoever maps workspaces).
pub const WORKSPACE_PROJECT: &str = "org.rsynapse.workspace.project";
/// Workspace display name (owned by the steward namer).
pub const WORKSPACE_NAME: &str = "org.rsynapse.workspace.name";
/// Window -> inner session (editor, agent, …), owned by external hooks.
pub const WINDOW_APP_INSTANCE: &str = "org.rsynapse.window.app-instance";
pub const WINDOW_AGENT_SESSION: &str = "org.rsynapse.window.agent-session";

/// Stable-key kind for workspace-name targets.
pub const WORKSPACE_NAME_KIND: &str = "org.rsynapse.workspace.name";

/// Thin imperative locus client. Reads go through [`records`] observables;
/// this is only the write path plus the one-shot reads the watch loop needs.
#[derive(Clone)]
pub struct LocusClient {
    proxy: RelationsProxy<'static>,
}

impl LocusClient {
    pub async fn set_with_persistence(
        &self,
        subject: RelationEndpoint,
        relation: &str,
        target: RelationEndpoint,
        metadata: HashMap<String, String>,
        persist: bool,
    ) -> anyhow::Result<()> {
        self.proxy
            .set_with_persistence(subject, relation, target, metadata, persist)
            .await?;
        Ok(())
    }
    pub async fn unset(
        &self,
        subject: RelationEndpoint,
        relation: &str,
        target: RelationEndpoint,
    ) -> anyhow::Result<()> {
        self.proxy.unset(subject, relation, target).await?;
        Ok(())
    }
    pub async fn connect() -> anyhow::Result<Self> {
        let connection = Connection::session().await?;
        let proxy = RelationsProxy::new(&connection).await?;
        Ok(Self { proxy })
    }

    pub async fn list(&self, relation: &str) -> anyhow::Result<Vec<RelationRecord>> {
        Ok(self.proxy.list(relation).await?)
    }
    pub async fn list_with_persistence(
        &self,
        relation: &str,
    ) -> anyhow::Result<Vec<locus::RelationState>> {
        Ok(self.proxy.list_with_persistence(relation).await?)
    }

    pub async fn set_one_with_persistence(
        &self,
        subject: RelationEndpoint,
        relation: &str,
        target: RelationEndpoint,
        metadata: HashMap<String, String>,
        persist: bool,
    ) -> anyhow::Result<()> {
        self.proxy
            .set_one_with_persistence(subject, relation, target, metadata, persist)
            .await?;
        Ok(())
    }
}

/// Live `List` snapshot for one relation, re-read on every matching
/// relation signal. Shared by descriptor key across plugins.
pub fn records(client: LocusClient, relation: &'static str) -> Observable<Vec<RelationRecord>> {
    shell_source::shared_by_key("steward.locus-records", relation, move || {
        let client = client.clone();
        from_task(move |sender| {
            let client = client.clone();
            async move {
                // Snapshot replies must not share a connection with bounded
                // signal queues that can fill while the snapshot is in flight.
                let watch = match LocusClient::connect().await {
                    Ok(watch) => watch,
                    Err(error) => {
                        let _ = sender.send(Err(error.to_string())).await;
                        return;
                    }
                };
                // Subscribe before the initial snapshot so policy edits during
                // startup cannot fall into the List/subscribe gap.
                let mut added = match watch.proxy.receive_relation_added().await {
                    Ok(stream) => stream,
                    Err(error) => {
                        let _ = sender
                            .send(Err(format!("subscribe RelationAdded: {error}")))
                            .await;
                        return;
                    }
                };
                let mut updated = match watch.proxy.receive_relation_updated().await {
                    Ok(stream) => stream,
                    Err(error) => {
                        let _ = sender
                            .send(Err(format!("subscribe RelationUpdated: {error}")))
                            .await;
                        return;
                    }
                };
                let mut removed = match watch.proxy.receive_relation_removed().await {
                    Ok(stream) => stream,
                    Err(error) => {
                        let _ = sender
                            .send(Err(format!("subscribe RelationRemoved: {error}")))
                            .await;
                        return;
                    }
                };
                let mut cleared = match watch.proxy.receive_relation_cleared().await {
                    Ok(stream) => stream,
                    Err(error) => {
                        let _ = sender
                            .send(Err(format!("subscribe RelationCleared: {error}")))
                            .await;
                        return;
                    }
                };
                if sender
                    .send(list_records(&client, relation).await)
                    .await
                    .is_err()
                {
                    return;
                }
                loop {
                    let changed = tokio::select! {
                        message = added.next() => {
                            let Some(message) = message else { return; };
                            message.args().is_ok_and(|args| args.record.relation == relation)
                        }
                        message = updated.next() => {
                            let Some(message) = message else { return; };
                            message.args().is_ok_and(|args| args.record.relation == relation)
                        }
                        message = removed.next() => {
                            let Some(message) = message else { return; };
                            message.args().is_ok_and(|args| args.record.relation == relation)
                        }
                        message = cleared.next() => {
                            let Some(message) = message else { return; };
                            message.args().is_ok_and(|args| args.relation == relation)
                        }
                    };
                    if changed {
                        if sender
                            .send(list_records(&client, relation).await)
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }
        })
        .distinct_until_changed()
        .box_it()
    })
}

async fn list_records(client: &LocusClient, relation: &str) -> Result<Vec<RelationRecord>, String> {
    client
        .list(relation)
        .await
        .map_err(|error| format!("locus list {relation}: {error}"))
}

#[cfg(test)]
mod backpressure;
