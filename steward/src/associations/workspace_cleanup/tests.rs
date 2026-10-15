use super::*;
use locus::RelationRecord;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use zbus::{interface, object_server::SignalContext};

fn ready(owner: &str, generation: u64, ids: &[&str]) -> Snapshot {
    Snapshot {
        owner: owner.into(),
        generation,
        connected: true,
        ids: ids.iter().map(|id| (*id).into()).collect(),
    }
}

#[test]
fn bootstrap_reorder_reset_and_owner_loss_have_distinct_lifetimes() {
    let candidates = HashSet::from(["DP-1:2".into(), "gone/é".into()]);
    let mut tracker = Reconciler::default();
    assert!(
        tracker
            .expired(&Snapshot::default(), &candidates)
            .is_empty()
    );
    let initial = ready(":1.1", 1, &["DP-1:2"]);
    assert_eq!(
        tracker.expired(&initial, &candidates),
        HashSet::from(["gone/é".into()])
    );
    tracker.commit(initial.clone());
    assert!(tracker.expired(&initial, &initial.ids).is_empty());
    // Same native ID in a new source generation or owner cannot inherit links.
    assert_eq!(
        tracker.expired(&ready(":1.1", 2, &["DP-1:2"]), &initial.ids),
        initial.ids
    );
    assert_eq!(
        tracker.expired(&ready(":1.2", 1, &["DP-1:2"]), &initial.ids),
        initial.ids
    );
    assert_eq!(
        tracker.expired(&Snapshot::default(), &HashSet::new()),
        initial.ids
    );
}

struct Store {
    records: Vec<RelationRecord>,
    fail_next: bool,
}
struct Relations(Arc<Mutex<Store>>);
#[interface(name = "org.rsynapse.Locus.Relations1")]
impl Relations {
    async fn list(&self, relation: &str) -> Vec<RelationRecord> {
        self.0
            .lock()
            .await
            .records
            .iter()
            .filter(|r| relation.is_empty() || r.relation == relation)
            .cloned()
            .collect()
    }
    async fn list_with_persistence(&self, relation: &str) -> Vec<locus::RelationState> {
        self.list(relation)
            .await
            .into_iter()
            .map(|record| locus::RelationState {
                persist: record.relation != crate::relations::WORKSPACE_PROJECT,
                record,
            })
            .collect()
    }
    async fn set_with_persistence(
        &self,
        subject: RelationEndpoint,
        relation: String,
        target: RelationEndpoint,
        metadata: HashMap<String, String>,
        persist: bool,
    ) -> locus::RelationState {
        assert_eq!(
            persist,
            relation != crate::relations::WORKSPACE_PROJECT,
            "epoch annotation must preserve explicit persistence"
        );
        let mut store = self.0.lock().await;
        let record = store
            .records
            .iter_mut()
            .find(|r| r.subject == subject && r.relation == relation && r.target == target)
            .unwrap();
        record.metadata = metadata;
        locus::RelationState {
            record: record.clone(),
            persist,
        }
    }
    async fn unset(
        &self,
        subject: RelationEndpoint,
        relation: String,
        target: RelationEndpoint,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
    ) -> zbus::fdo::Result<bool> {
        let removed = {
            let mut store = self.0.lock().await;
            if store.fail_next {
                store.fail_next = false;
                return Err(zbus::fdo::Error::Failed(
                    "injected persistence failure".into(),
                ));
            }
            let removed: Vec<_> = store
                .records
                .iter()
                .filter(|r| r.subject == subject && r.relation == relation && r.target == target)
                .cloned()
                .collect();
            store.records.retain(|r| {
                !(r.subject == subject && r.relation == relation && r.target == target)
            });
            removed
        };
        for record in &removed {
            Self::relation_removed(&ctx, record.clone()).await?;
        }
        Ok(!removed.is_empty())
    }
    #[zbus(signal)]
    async fn relation_removed(ctx: &SignalContext<'_>, record: RelationRecord) -> zbus::Result<()>;
}

struct Manager(Arc<Mutex<Vec<OwnedObjectPath>>>);
#[interface(name = "org.rsynapse.Workspace.Manager1")]
impl Manager {
    #[zbus(property)]
    fn connected(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn generation(&self) -> u64 {
        1
    }
    #[zbus(property)]
    async fn workspaces(&self) -> Vec<OwnedObjectPath> {
        self.0.lock().await.clone()
    }
}
struct Workspace(String);
#[interface(name = "org.rsynapse.Workspace.Workspace1")]
impl Workspace {
    #[zbus(property(emits_changed_signal = "const"))]
    fn id(&self) -> &str {
        &self.0
    }
}

fn record(subject: RelationEndpoint, relation: &str) -> RelationRecord {
    RelationRecord {
        subject,
        relation: relation.into(),
        target: RelationEndpoint::stable_key(locus::keys::PROJECT_PATH, "/project"),
        metadata: HashMap::from([("project-id".into(), "project-survives".into())]),
        created_at_unix_ms: 1,
        updated_at_unix_ms: 1,
    }
}
fn subject(id: &str) -> RelationEndpoint {
    RelationEndpoint::stable_key(locus::keys::WORKSPACE_ID, id)
}

#[test]
fn workspace_lifecycle_cleans_associations_and_preserves_projects_and_goals() {
    if std::env::var_os("WORKSPACE_CLEANUP_WORKER").is_none() {
        let status = std::process::Command::new("dbus-run-session").arg("--").arg(std::env::current_exe().unwrap())
            .args(["--exact", "associations::workspace_cleanup::tests::workspace_lifecycle_cleans_associations_and_preserves_projects_and_goals", "--nocapture"])
            .env("WORKSPACE_CLEANUP_WORKER", "1").status().unwrap();
        assert!(status.success());
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        tokio::time::timeout(Duration::from_secs(10), exercise())
            .await
            .expect("cleanup stopped making progress")
            .unwrap();
    });
}

async fn exercise() -> anyhow::Result<()> {
    let project = record(
        RelationEndpoint::stable_key(locus::keys::PROJECT_PATH, "/project"),
        "project.goal",
    );
    let goal = record(
        RelationEndpoint::stable_key(goal_model::GOAL_KIND, "goal-survives"),
        "goal.project",
    );
    let named = record(
        RelationEndpoint::stable_key(locus::keys::WORKSPACE_NAME, "coding"),
        "workspace.icon",
    );
    let store = Arc::new(Mutex::new(Store {
        records: vec![
            record(
                subject("remove/é"),
                super::super::relations::WORKSPACE_PROJECT,
            ),
            record(subject("owner-loss"), "workspace.project"),
            record(subject("stale"), "workspace.project"),
            project.clone(),
            goal.clone(),
            named.clone(),
        ],
        fail_next: false,
    }));
    let locus_service = zbus::connection::Builder::session()?
        .serve_at(locus::OBJECT_PATH, Relations(store.clone()))?
        .name(locus::BUS_NAME)?
        .build()
        .await?;
    let paths = Arc::new(Mutex::new(vec![
        workspace_dbus_api::workspace_path("remove/é"),
        workspace_dbus_api::workspace_path("owner-loss"),
    ]));
    let source = zbus::connection::Builder::session()?
        .serve_at(ROOT_PATH, zbus::fdo::ObjectManager)?
        .serve_at(ROOT_PATH, Manager(paths.clone()))?
        .serve_at(
            workspace_dbus_api::workspace_path("remove/é"),
            Workspace("remove/é".into()),
        )?
        .serve_at(
            workspace_dbus_api::workspace_path("owner-loss"),
            Workspace("owner-loss".into()),
        )?
        .name(BUS_NAME)?
        .build()
        .await?;
    let client = LocusClient::connect().await?;
    let observer = zbus::Proxy::new(
        &locus_service,
        locus::BUS_NAME,
        locus::OBJECT_PATH,
        locus::RELATIONS_INTERFACE,
    )
    .await?;
    let mut removals = observer.receive_signal("RelationRemoved").await?;
    let worker = tokio::spawn(run(client.clone()));
    let removed: RelationRecord = removals.next().await.unwrap().body().deserialize()?;
    assert_eq!(removed.subject, subject("stale"));
    assert!(
        store
            .lock()
            .await
            .records
            .iter()
            .any(|r| r.subject == subject("remove/é"))
    );

    paths.lock().await.remove(0);
    let manager = source
        .object_server()
        .interface::<_, Manager>(ROOT_PATH)
        .await?;
    manager
        .get()
        .await
        .workspaces_changed(manager.signal_context())
        .await?;
    source
        .object_server()
        .remove::<Workspace, _>(workspace_dbus_api::workspace_path("remove/é"))
        .await?;
    let removed: RelationRecord = removals.next().await.unwrap().body().deserialize()?;
    assert_eq!(removed.subject, subject("remove/é"));
    assert!(
        store
            .lock()
            .await
            .records
            .iter()
            .any(|r| r.subject == subject("owner-loss"))
    );

    source.release_name(BUS_NAME).await?;
    let removed: RelationRecord = removals.next().await.unwrap().body().deserialize()?;
    assert_eq!(removed.subject, subject("owner-loss"));
    assert_eq!(store.lock().await.records, [project, goal, named]);
    worker.abort();

    // Failed durable cleanup cannot advance the tracker and lose a retry.
    let mut tracker = Reconciler::default();
    tracker.commit(ready(":1.test", 1, &["retry"]));
    store
        .lock()
        .await
        .records
        .push(record(subject("retry"), "workspace.project"));
    store.lock().await.fail_next = true;
    let after = ready(":1.test", 1, &[]);
    assert!(
        reconcile(&client, &mut tracker, after.clone())
            .await
            .is_err()
    );
    assert!(tracker.known.contains("retry"));
    reconcile(&client, &mut tracker, after).await?;
    assert!(!tracker.known.contains("retry"));
    assert!(
        !store
            .lock()
            .await
            .records
            .iter()
            .any(|r| r.subject == subject("retry"))
    );

    // A bind stamped after a source reset wins over stale outgoing relations
    // for a reused native ID. This also survives restarting the reconciler.
    let mut old = record(subject("same-id"), "workspace.name");
    stamp(&mut old.metadata, (":1.old".into(), "1".into()));
    let mut fresh = record(subject("same-id"), "workspace.project");
    stamp(&mut fresh.metadata, (":1.new".into(), "2".into()));
    store.lock().await.records.extend([old, fresh.clone()]);
    reconcile(
        &client,
        &mut Reconciler::default(),
        ready(":1.new", 2, &["same-id"]),
    )
    .await?;
    let same: Vec<_> = store
        .lock()
        .await
        .records
        .iter()
        .filter(|r| r.subject == subject("same-id"))
        .cloned()
        .collect();
    assert_eq!(same, [fresh]);
    Ok(())
}
