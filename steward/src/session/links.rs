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
pub const OUTPUT_PREFERENCE: &str = "org.rsynapse.session.workspace.preferred-output";

pub fn preferred_output(records: &[RelationRecord], slot: &str) -> Option<(String, u32, bool)> {
    let record = target(records, OUTPUT_PREFERENCE, SLOT, slot)?;
    let output = key(&record.target, locus::keys::OUTPUT_NAME)?;
    let index = record.metadata.get("index")?.parse::<u32>().ok()?;
    (index > 0 && index <= 64).then(|| {
        (
            output,
            index,
            record.metadata.get("fallback").is_some_and(|v| v == "true"),
        )
    })
}

pub async fn set_preferred_output(
    locus: &LocusClient,
    records: &[RelationRecord],
    slot: &str,
    output: &str,
    index: u32,
    fallback: bool,
) -> anyhow::Result<()> {
    let mut meta = metadata(None);
    meta.insert("index".into(), index.to_string());
    meta.insert("fallback".into(), fallback.to_string());
    set(
        locus,
        records,
        OUTPUT_PREFERENCE,
        RelationEndpoint::stable_key(SLOT, slot),
        RelationEndpoint::stable_key(locus::keys::OUTPUT_NAME, output),
        meta,
        true,
    )
    .await
}

/// A logical workspace has one live location in a source epoch. When returning
/// from fallback, retire the old physical binding even if a disabled output
/// retains its empty workspace object.
pub async fn bind_slot(
    locus: &LocusClient,
    source: &Snapshot,
    slot: &str,
    live: &str,
) -> anyhow::Result<()> {
    let records = locus.list("").await?;
    let desired = RelationEndpoint::stable_key(SLOT, slot);
    let subject = RelationEndpoint::stable_key(locus::keys::WORKSPACE_ID, live);
    for record in &records {
        if record.relation != LIVE_SLOT
            || record.target != desired
            || record.subject == subject
            || !current(record, source)
        {
            continue;
        }
        for old in &records {
            if old.subject == record.subject
                && [
                    LIVE_SLOT,
                    crate::relations::WORKSPACE_PROJECT,
                    "org.rsynapse.workspace.project-policy",
                ]
                .contains(&old.relation.as_str())
            {
                locus
                    .unset(old.subject.clone(), &old.relation, old.target.clone())
                    .await?;
            }
        }
        if key(&record.subject, locus::keys::WORKSPACE_ID)
            .is_some_and(|id| source.workspaces.contains_key(&id))
        {
            let mut meta = metadata(Some(source));
            meta.insert("automatic".into(), "false".into());
            locus
                .set_one_with_persistence(
                    record.subject.clone(),
                    "org.rsynapse.workspace.project-policy",
                    RelationEndpoint::stable_key("org.rsynapse.project-assignment", "unassigned"),
                    meta,
                    false,
                )
                .await?;
        }
    }
    locus
        .set_one_with_persistence(subject, LIVE_SLOT, desired, metadata(Some(source)), false)
        .await
}

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
