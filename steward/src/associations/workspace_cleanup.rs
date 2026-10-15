//! Workspace lifetime policy belongs here, not in the compositor projection.
use crate::relations::LocusClient;
use futures_util::StreamExt;
use locus::RelationEndpoint;
use std::collections::HashSet;
use workspace_dbus_api::{BUS_NAME, MANAGER_INTERFACE, ROOT_PATH, WORKSPACE_INTERFACE};
use zbus::{
    Connection, MatchRule, MessageStream,
    fdo::{DBusProxy, ObjectManagerProxy, PropertiesProxy},
    message::Type,
    zvariant::OwnedObjectPath,
};

const OWNER: &str = "workspace-source-owner";
const GENERATION: &str = "workspace-source-generation";

fn source_changed(message: &zbus::Message) -> bool {
    let header = message.header();
    match header.member().map(|name| name.as_str()) {
        Some("PropertiesChanged") if header.path().is_some_and(|p| p.as_str() == ROOT_PATH) => {
            let Ok((interface, changed, invalidated)) = message.body().deserialize::<(
                String,
                std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
                Vec<String>,
            )>() else {
                return true;
            };
            interface == MANAGER_INTERFACE
                && ["Connected", "Generation", "Workspaces"]
                    .iter()
                    .any(|name| {
                        changed.contains_key(*name)
                            || invalidated.iter().any(|value| value.as_str() == *name)
                    })
        }
        Some("InterfacesAdded") => {
            let Ok((_, interfaces)) = message.body().deserialize::<(
                OwnedObjectPath,
                std::collections::HashMap<
                    String,
                    std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
                >,
            )>() else {
                return true;
            };
            interfaces.contains_key(WORKSPACE_INTERFACE)
        }
        Some("InterfacesRemoved") => {
            let Ok((_, interfaces)) = message
                .body()
                .deserialize::<(OwnedObjectPath, Vec<String>)>()
            else {
                return true;
            };
            interfaces.iter().any(|name| name == WORKSPACE_INTERFACE)
        }
        _ => false,
    }
}

fn relation_changed(message: &zbus::Message, tracker: &Reconciler) -> bool {
    let header = message.header();
    if !matches!(
        header.member().map(|name| name.as_str()),
        Some("RelationAdded" | "RelationUpdated")
    ) {
        return false;
    }
    let Ok(record) = message.body().deserialize::<locus::RelationRecord>() else {
        return true;
    };
    let RelationEndpoint::StableKey { kind, id } = &record.subject else {
        return false;
    };
    if kind != locus::keys::WORKSPACE_ID {
        return false;
    }
    // Our own epoch annotation already describes an observed live workspace.
    // It need not cause another full inventory/Locus read.
    !tracker.epoch.as_ref().is_some_and(|(owner, generation)| {
        tracker.known.contains(id)
            && record.metadata.get(OWNER) == Some(owner)
            && record.metadata.get(GENERATION) == Some(&generation.to_string())
    })
}

#[derive(Clone, Debug, Default)]
struct Snapshot {
    owner: String,
    generation: u64,
    connected: bool,
    ids: HashSet<String>,
}

#[derive(Default)]
struct Reconciler {
    epoch: Option<(String, u64)>,
    known: HashSet<String>,
}
impl Reconciler {
    fn expired(&self, snapshot: &Snapshot, candidates: &HashSet<String>) -> HashSet<String> {
        // An initial disconnected/bootstrap view is not evidence of removal.
        if !snapshot.connected && self.epoch.is_none() {
            return HashSet::new();
        }
        let reset = !snapshot.connected
            || self.epoch.as_ref().is_some_and(|(owner, generation)| {
                owner != &snapshot.owner || *generation != snapshot.generation
            });
        self.known
            .union(candidates)
            .filter(|id| reset || !snapshot.ids.contains(*id))
            .cloned()
            .collect()
    }
    fn commit(&mut self, snapshot: Snapshot) {
        if snapshot.connected {
            self.epoch = Some((snapshot.owner, snapshot.generation));
            self.known = snapshot.ids;
        } else {
            self.known.clear();
        }
    }
}

#[derive(PartialEq)]
struct Root {
    connected: bool,
    generation: u64,
    paths: Vec<OwnedObjectPath>,
}
async fn root(proxy: &PropertiesProxy<'_>) -> anyhow::Result<Root> {
    let mut props = proxy
        .get_all(Some(MANAGER_INTERFACE.try_into()?).into())
        .await?;
    Ok(Root {
        connected: bool::try_from(
            props
                .remove("Connected")
                .ok_or_else(|| anyhow::anyhow!("missing Connected"))?,
        )?,
        generation: u64::try_from(
            props
                .remove("Generation")
                .ok_or_else(|| anyhow::anyhow!("missing Generation"))?,
        )?,
        paths: Vec::try_from(
            props
                .remove("Workspaces")
                .ok_or_else(|| anyhow::anyhow!("missing Workspaces"))?,
        )?,
    })
}

async fn snapshot(connection: &Connection) -> anyhow::Result<Option<Snapshot>> {
    let bus = DBusProxy::new(connection).await?;
    let owner = match bus.get_name_owner(BUS_NAME.try_into()?).await {
        Ok(owner) => owner.to_string(),
        Err(zbus::fdo::Error::NameHasNoOwner(_)) => return Ok(Some(Snapshot::default())),
        Err(error) => return Err(error.into()),
    };
    // Pin all reads to one unique bus owner, not a replaceable well-known name.
    let properties = PropertiesProxy::builder(connection)
        .destination(owner.as_str())?
        .path(ROOT_PATH)?
        .build()
        .await?;
    let before = match root(&properties).await {
        Ok(root) => root,
        // Owner loss/reset is followed by its subscribed lifecycle event.
        Err(error) => {
            eprintln!("[steward/workspace-cleanup] source read interrupted: {error}");
            return Ok(None);
        }
    };
    if !before.connected {
        return Ok(Some(Snapshot {
            owner,
            generation: before.generation,
            ..Snapshot::default()
        }));
    }
    let objects = match ObjectManagerProxy::builder(connection)
        .destination(owner.as_str())?
        .path(ROOT_PATH)?
        .build()
        .await?
        .get_managed_objects()
        .await
    {
        Ok(objects) => objects,
        Err(error) => {
            eprintln!("[steward/workspace-cleanup] inventory read interrupted: {error}");
            return Ok(None);
        }
    };
    let projected: HashSet<_> = objects
        .iter()
        .filter(|(_, interfaces)| {
            interfaces
                .keys()
                .any(|name| name.as_str() == WORKSPACE_INTERFACE)
        })
        .map(|(path, _)| path.clone())
        .collect();
    if projected != before.paths.iter().cloned().collect() {
        return Ok(None);
    }
    let mut ids = HashSet::new();
    for path in &before.paths {
        let Some(value) = objects
            .get(path)
            .and_then(|interfaces| {
                interfaces
                    .iter()
                    .find(|(name, _)| name.as_str() == WORKSPACE_INTERFACE)
            })
            .and_then(|(_, props)| props.get("Id"))
        else {
            return Ok(None);
        };
        let id = String::try_from(value.try_clone()?)?;
        anyhow::ensure!(!id.is_empty(), "workspace has an empty Id");
        ids.insert(id);
    }
    let after = match root(&properties).await {
        Ok(root) => root,
        Err(_) => return Ok(None),
    };
    let current_owner = match bus.get_name_owner(BUS_NAME.try_into()?).await {
        Ok(owner) => owner,
        Err(zbus::fdo::Error::NameHasNoOwner(_)) => return Ok(Some(Snapshot::default())),
        Err(error) => return Err(error.into()),
    };
    if after != before || current_owner.as_str() != owner {
        // Inventory changed while reading; queued signals trigger another read.
        return Ok(None);
    }
    Ok(Some(Snapshot {
        owner,
        generation: before.generation,
        connected: true,
        ids,
    }))
}

async fn reconcile(
    locus: &LocusClient,
    tracker: &mut Reconciler,
    snapshot: Snapshot,
) -> anyhow::Result<()> {
    let states = locus.list_with_persistence("").await?;
    let candidates = states
        .iter()
        .filter_map(|state| match &state.record.subject {
            RelationEndpoint::StableKey { kind, id } if kind == locus::keys::WORKSPACE_ID => {
                Some(id.clone())
            }
            _ => None,
        })
        .collect();
    let legacy_expired = tracker.expired(&snapshot, &candidates);
    let generation = snapshot.generation.to_string();
    for state in states {
        let record = state.record;
        let RelationEndpoint::StableKey { kind, id } = &record.subject else {
            continue;
        };
        if kind != locus::keys::WORKSPACE_ID {
            continue;
        }
        let expired = if let (Some(owner), Some(epoch)) =
            (record.metadata.get(OWNER), record.metadata.get(GENERATION))
        {
            if snapshot.connected {
                !snapshot.ids.contains(id) || owner != &snapshot.owner || epoch != &generation
            } else {
                tracker.epoch.is_some()
            }
        } else {
            legacy_expired.contains(id)
        };
        if expired {
            // Per-record removal preserves a freshly rebound current-generation
            // project even if old relations for the same native ID still exist.
            locus
                .unset(record.subject, &record.relation, record.target)
                .await?;
        } else if snapshot.connected
            && snapshot.ids.contains(id)
            && (record.metadata.get(OWNER) != Some(&snapshot.owner)
                || record.metadata.get(GENERATION) != Some(&generation))
        {
            let mut metadata = record.metadata;
            metadata.insert(OWNER.into(), snapshot.owner.clone());
            metadata.insert(GENERATION.into(), generation.clone());
            locus
                .set_with_persistence(
                    record.subject,
                    &record.relation,
                    record.target,
                    metadata,
                    state.persist,
                )
                .await?;
        }
    }
    // Advance only after every acknowledged cleanup, so failures remain retryable.
    tracker.commit(snapshot);
    Ok(())
}

/// Called while holding BIND_LOCK before creating a new project association.
/// Reconcile old epochs first so background cleanup cannot erase a fresh bind.
pub(super) async fn synchronize(locus: &LocusClient) -> anyhow::Result<(String, String)> {
    let connection = Connection::session().await?;
    let snapshot = snapshot(&connection)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Workspace inventory is changing"))?;
    anyhow::ensure!(snapshot.connected, "Workspace source is disconnected");
    let epoch = (snapshot.owner.clone(), snapshot.generation.to_string());
    reconcile(locus, &mut Reconciler::default(), snapshot).await?;
    Ok(epoch)
}

pub(super) fn stamp(
    metadata: &mut std::collections::HashMap<String, String>,
    epoch: (String, String),
) {
    metadata.insert(OWNER.into(), epoch.0);
    metadata.insert(GENERATION.into(), epoch.1);
}

pub(super) async fn run(locus: LocusClient) -> anyhow::Result<()> {
    let connection = Connection::session().await?;
    let mut source = MessageStream::for_match_rule(
        MatchRule::builder()
            .msg_type(Type::Signal)
            .sender(BUS_NAME)?
            .path_namespace(ROOT_PATH)?
            .build(),
        &connection,
        Some(128),
    )
    .await?;
    let bus = DBusProxy::new(&connection).await?;
    let mut owners = bus.receive_name_owner_changed().await?;
    let mut relations = MessageStream::for_match_rule(
        MatchRule::builder()
            .msg_type(Type::Signal)
            .sender(locus::BUS_NAME)?
            .interface(locus::RELATIONS_INTERFACE)?
            .build(),
        &connection,
        Some(128),
    )
    .await?;
    let mut tracker = Reconciler::default();
    loop {
        {
            let _lock = super::BIND_LOCK.lock().await;
            if let Some(snapshot) = snapshot(&connection).await? {
                reconcile(&locus, &mut tracker, snapshot).await?;
            }
        }
        loop {
            let changed = tokio::select! {
                message = source.next() => source_changed(&message.ok_or_else(|| anyhow::anyhow!("workspace signal stream ended"))??),
                message = relations.next() => relation_changed(&message.ok_or_else(|| anyhow::anyhow!("Locus signal stream ended"))??, &tracker),
                message = owners.next() => {
                    let message = message.ok_or_else(|| anyhow::anyhow!("bus owner stream ended"))?;
                    let args = message.args()?;
                    [BUS_NAME, locus::BUS_NAME].contains(&args.name().as_str())
                }
            };
            if changed {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests;
