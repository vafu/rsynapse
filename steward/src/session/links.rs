use super::observe::Snapshot;
use crate::relations::LocusClient;
use locus::{RelationEndpoint, RelationRecord};
use std::collections::HashMap;

pub const SLOT: &str = "org.rsynapse.session.workspace.id";
pub const CHECKPOINT: &str = "org.rsynapse.session.workspace-checkpoint.id";
pub const SAVED_WINDOW: &str = "org.rsynapse.session.window.id";
pub const LIVE_SLOT: &str = "org.rsynapse.workspace.session-workspace";
pub const LIVE_WINDOW: &str = "org.rsynapse.window.session-window";
pub const SLOT_PROJECT: &str = "org.rsynapse.session.workspace.project";
pub const CHECKPOINT_PROJECT: &str = "org.rsynapse.session.checkpoint.project";
pub const CAPTURE: &str = "org.rsynapse.session.capture";
pub const PROJECT: &str = "org.rsynapse.project.id";

pub fn metadata(source: Option<&Snapshot>) -> HashMap<String, String> {
    let mut result = HashMap::from([("managed-by".into(), "rsynapse-steward".into())]);
    if let Some(source) = source {
        result.insert("workspace-source-owner".into(), source.owner.clone());
        result.insert(
            "workspace-source-generation".into(),
            source.generation.to_string(),
        );
    }
    result
}
pub fn current(record: &RelationRecord, source: &Snapshot) -> bool {
    let expected = metadata(Some(source));
    expected
        .iter()
        .all(|(key, value)| record.metadata.get(key) == Some(value))
}
pub fn target<'a>(
    records: &'a [RelationRecord],
    relation: &str,
    kind: &str,
    id: &str,
) -> Option<&'a RelationRecord> {
    let subject = RelationEndpoint::stable_key(kind, id);
    records
        .iter()
        .find(|r| r.relation == relation && r.subject == subject)
}
pub fn key(endpoint: &RelationEndpoint, expected: &str) -> Option<String> {
    match endpoint {
        RelationEndpoint::StableKey { kind, id } if kind == expected => Some(id.clone()),
        _ => None,
    }
}

pub async fn set(
    locus: &LocusClient,
    records: &[RelationRecord],
    relation: &str,
    subject: RelationEndpoint,
    target: RelationEndpoint,
    metadata: HashMap<String, String>,
    persist: bool,
) -> anyhow::Result<()> {
    if records.iter().any(|r| {
        r.relation == relation
            && r.subject == subject
            && r.target == target
            && r.metadata == metadata
    }) {
        return Ok(());
    }
    locus
        .set_one_with_persistence(subject, relation, target, metadata, persist)
        .await
}

pub async fn live(
    locus: &LocusClient,
    records: &[RelationRecord],
    source: &Snapshot,
    relation: &str,
    source_kind: &str,
    live_id: &str,
    saved_kind: &str,
) -> anyhow::Result<String> {
    if let Some(record) =
        target(records, relation, source_kind, live_id).filter(|r| current(r, source))
    {
        if let Some(id) = key(&record.target, saved_kind) {
            return Ok(id);
        }
    }
    let id = super::model::id();
    set(
        locus,
        records,
        relation,
        RelationEndpoint::stable_key(source_kind, live_id),
        RelationEndpoint::stable_key(saved_kind, &id),
        metadata(Some(source)),
        false,
    )
    .await?;
    Ok(id)
}

pub fn epoch(source: &Snapshot) -> RelationEndpoint {
    RelationEndpoint::stable_key(
        "org.rsynapse.session.source-epoch",
        format!("{}/{}", source.owner, source.generation),
    )
}

pub async fn arm(locus: &LocusClient, source: &Snapshot) -> anyhow::Result<()> {
    locus
        .set_one_with_persistence(
            RelationEndpoint::stable_key("org.rsynapse.session.capture", "active"),
            CAPTURE,
            epoch(source),
            metadata(Some(source)),
            false,
        )
        .await
}
