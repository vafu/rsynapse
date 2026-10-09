use std::collections::HashMap;

use futures_util::StreamExt;
use locus::{RelationEndpoint, RelationRecord};
use shell_source::{Observable, from_task, rx::Observable as _};
use zbus::{Connection, Proxy};

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
    proxy: Proxy<'static>,
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
            .call::<_, _, locus::RelationState>(
                "SetWithPersistence",
                &(subject, relation, target, metadata, persist),
            )
            .await?;
        Ok(())
    }
    pub async fn unset(
        &self,
        subject: RelationEndpoint,
        relation: &str,
        target: RelationEndpoint,
    ) -> anyhow::Result<()> {
        self.proxy
            .call::<_, _, bool>("Unset", &(subject, relation, target))
            .await?;
        Ok(())
    }
    pub async fn connect() -> anyhow::Result<Self> {
        let connection = Connection::session().await?;
        let proxy = Proxy::new_owned(
            connection,
            locus::BUS_NAME,
            locus::OBJECT_PATH,
            locus::RELATIONS_INTERFACE,
        )
        .await?;
        Ok(Self { proxy })
    }

    pub async fn list(&self, relation: &str) -> anyhow::Result<Vec<RelationRecord>> {
        Ok(self.proxy.call("List", &(relation,)).await?)
    }
    pub async fn list_with_persistence(
        &self,
        relation: &str,
    ) -> anyhow::Result<Vec<locus::RelationState>> {
        Ok(self.proxy.call("ListWithPersistence", &(relation,)).await?)
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
            .call::<_, _, locus::RelationState>(
                "SetOneWithPersistence",
                &(subject, relation, target, metadata, persist),
            )
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
                // Subscribe before the initial snapshot so policy edits during
                // startup cannot fall into the List/subscribe gap.
                let mut added = Box::pin(signal_stream(&client, "RelationAdded").await);
                let mut updated = Box::pin(signal_stream(&client, "RelationUpdated").await);
                let mut removed = Box::pin(signal_stream(&client, "RelationRemoved").await);
                let mut cleared = Box::pin(signal_stream(&client, "RelationCleared").await);
                if sender
                    .send(list_records(&client, relation).await)
                    .await
                    .is_err()
                {
                    return;
                }
                loop {
                    let refresh = tokio::select! {
                        message = added.next() => message,
                        message = updated.next() => message,
                        message = removed.next() => message,
                        message = cleared.next() => message,
                    };
                    let Some(message) = refresh else {
                        return;
                    };
                    if message_matches(&message, relation) {
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

async fn signal_stream<'a>(
    client: &'a LocusClient,
    member: &'a str,
) -> futures_util::stream::BoxStream<'a, zbus::Message> {
    use futures_util::StreamExt;
    match client.proxy.receive_signal(member).await {
        Ok(stream) => stream.boxed(),
        Err(_) => futures_util::stream::empty().boxed(),
    }
}

fn message_matches(message: &zbus::Message, relation: &str) -> bool {
    let member = message
        .header()
        .member()
        .map(|member| member.to_string())
        .unwrap_or_default();
    match member.as_str() {
        "RelationAdded" | "RelationUpdated" | "RelationRemoved" => message
            .body()
            .deserialize::<RelationRecord>()
            .map(|record| record.relation == relation)
            .unwrap_or(false),
        "RelationCleared" => message
            .body()
            .deserialize::<(RelationEndpoint, String, u32)>()
            .map(|(_, cleared, _)| cleared == relation)
            .unwrap_or(false),
        _ => false,
    }
}
