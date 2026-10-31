use super::*;
use std::sync::Arc;
use tokio::sync::Mutex;
use zbus::zvariant::OwnedObjectPath;

fn project() -> goal_model::ProjectInfo {
    goal_model::ProjectInfo {
        id: "observed-project".into(),
        name: "Observed".into(),
        cwd: "/known/project".into(),
        checkout_id: String::new(),
        icon: String::new(),
        icon_origin: String::new(),
    }
}

struct Projects(Arc<Mutex<usize>>);
#[interface(name = "org.rsynapse.Proj.Manager1")]
impl Projects {
    fn resolve_project(&self, path: &str) -> zbus::fdo::Result<goal_model::ProjectInfo> {
        if path.starts_with("/known/project/") || path == "/known/project" {
            Ok(project())
        } else {
            Err(zbus::fdo::Error::InvalidArgs("unknown project".into()))
        }
    }
    async fn register_project(&self, path: &str) -> zbus::fdo::Result<goal_model::ProjectInfo> {
        *self.0.lock().await += 1;
        self.resolve_project(path)
    }
    fn list_checkouts(&self) -> Vec<goal_model::CheckoutInfo> {
        vec![]
    }
}

struct Relations(Arc<Mutex<Vec<locus::RelationRecord>>>);
#[interface(name = "org.rsynapse.Locus.Relations1")]
impl Relations {
    async fn list(&self, relation: &str) -> Vec<locus::RelationRecord> {
        self.0
            .lock()
            .await
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
                record,
                persist: false,
            })
            .collect()
    }
    async fn set_one_with_persistence(
        &self,
        subject: RelationEndpoint,
        relation: String,
        target: RelationEndpoint,
        metadata: HashMap<String, String>,
        persist: bool,
    ) -> locus::RelationState {
        assert!(!persist);
        let mut records = self.0.lock().await;
        records.retain(|r| r.subject != subject || r.relation != relation);
        let record = locus::RelationRecord {
            subject,
            relation,
            target,
            metadata,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
        };
        records.push(record.clone());
        locus::RelationState { record, persist }
    }
    async fn set_with_persistence(
        &self,
        subject: RelationEndpoint,
        relation: String,
        target: RelationEndpoint,
        metadata: HashMap<String, String>,
        persist: bool,
    ) -> locus::RelationState {
        self.set_one_with_persistence(subject, relation, target, metadata, persist)
            .await
    }
    async fn unset(
        &self,
        subject: RelationEndpoint,
        relation: String,
        target: RelationEndpoint,
    ) -> bool {
        let mut records = self.0.lock().await;
        let before = records.len();
        records.retain(|r| r.subject != subject || r.relation != relation || r.target != target);
        before != records.len()
    }
}

struct Workspaces;
#[interface(name = "org.rsynapse.Workspace.Manager1")]
impl Workspaces {
    #[zbus(property)]
    fn connected(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn generation(&self) -> u64 {
        1
    }
    #[zbus(property)]
    fn workspaces(&self) -> Vec<OwnedObjectPath> {
        vec![
            workspace_dbus_api::workspace_path("chosen"),
            workspace_dbus_api::workspace_path("unassigned"),
        ]
    }
}
struct Workspace(&'static str);
#[interface(name = "org.rsynapse.Workspace.Workspace1")]
impl Workspace {
    #[zbus(property(emits_changed_signal = "const"))]
    fn id(&self) -> &str {
        self.0
    }
}

#[test]
fn observed_cwd_only_attributes_windows_and_explicit_workspace_choices_win() {
    if std::env::var_os("STEWARD_ASSOCIATION_WORKER").is_none() {
        assert!(std::process::Command::new("dbus-run-session").arg("--")
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", "associations::tests::observed_cwd_only_attributes_windows_and_explicit_workspace_choices_win", "--nocapture"])
            .env("STEWARD_ASSOCIATION_WORKER", "1").status().unwrap().success());
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(10), exercise())
            .await
            .unwrap()
            .unwrap();
    });
}

async fn exercise() -> anyhow::Result<()> {
    let chosen = locus::RelationRecord {
        subject: RelationEndpoint::stable_key(locus::keys::WORKSPACE_ID, "chosen"),
        relation: relations::WORKSPACE_PROJECT.into(),
        target: RelationEndpoint::stable_key(locus::keys::PROJECT_PATH, "/explicit/project"),
        metadata: HashMap::from([("project-id".into(), "explicit-project".into())]),
        created_at_unix_ms: 1,
        updated_at_unix_ms: 1,
    };
    let records = Arc::new(Mutex::new(vec![chosen.clone()]));
    let registrations = Arc::new(Mutex::new(0));
    let service = zbus::connection::Builder::session()?
        .serve_at(locus::OBJECT_PATH, Relations(records.clone()))?
        .serve_at(goal_model::ROOT_PATH, Projects(registrations.clone()))?
        .name(locus::BUS_NAME)?
        .name(goal_model::BUS_NAME)?
        .build()
        .await?;
    let client = LocusClient::connect().await?;

    // This observed-window path works without any workspace or session service.
    bind_window(&client, "/known/project/subdirectory", "window".into()).await?;
    assert_eq!(client.list(relations::WORKSPACE_PROJECT).await?, [chosen]);
    assert_eq!(
        client.list(WINDOW_PROJECT).await?[0].metadata["project-id"],
        project().id
    );
    assert_eq!(*registrations.lock().await, 0);
    assert!(
        bind_window(&client, "/unknown", "unknown-window".into())
            .await
            .is_err()
    );
    assert_eq!(*registrations.lock().await, 0);

    service
        .object_server()
        .at(workspace_dbus_api::ROOT_PATH, zbus::fdo::ObjectManager)
        .await?;
    service
        .object_server()
        .at(workspace_dbus_api::ROOT_PATH, Workspaces)
        .await?;
    for id in ["chosen", "unassigned"] {
        service
            .object_server()
            .at(workspace_dbus_api::workspace_path(id), Workspace(id))
            .await?;
    }
    service.request_name(workspace_dbus_api::BUS_NAME).await?;

    // Explicit initialization can bind an empty workspace. Removing that choice
    // is sufficient: later agent observations must not need a blocking tombstone.
    bind_workspace(&client, "/known/project", "unassigned".into()).await?;
    assert_eq!(*registrations.lock().await, 1);
    Bindings {
        locus: client.clone(),
    }
    .unassign_workspace_project("unassigned".into())
    .await?;
    bind_window(&client, "/known/project", "window".into()).await?;
    assert!(!client.list("").await?.iter().any(
        |r| r.subject == RelationEndpoint::stable_key(locus::keys::WORKSPACE_ID, "unassigned")
    ));
    assert_eq!(client.list(relations::WORKSPACE_PROJECT).await?.len(), 1);
    Ok(())
}
