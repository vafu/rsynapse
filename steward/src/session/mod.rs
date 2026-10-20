//! Project-aware desktop capture and restoration. No project CRUD or compositor
//! protocol lives here; Locus owns associations, the projection owns controls.
mod capture;
pub mod cli;
mod launch;
mod links;
mod model;
mod observe;
mod restore;
mod store;
#[cfg(test)]
mod tests;
mod topology;

use crate::relations::LocusClient;
use futures_util::StreamExt;
use model::{RestoreReport, Session};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::Mutex;
use zbus::{Connection, Proxy, interface};

const ROOT: &str = "/org/rsynapse/Steward";
const BUS: &str = "org.rsynapse.Steward";
const INTERFACE: &str = "org.rsynapse.Steward.Session1";

pub struct Manager {
    connection: Connection,
    locus: LocusClient,
    store: Mutex<store::Store>,
    rules: launch::Rules,
    operations: Mutex<()>,
    jobs: Mutex<BTreeMap<String, RestoreReport>>,
    baseline: Mutex<Option<topology::Baseline>>,
}
impl Manager {
    pub async fn new(locus: LocusClient) -> anyhow::Result<Arc<Self>> {
        Ok(Arc::new(Self {
            connection: Connection::session().await?,
            locus,
            store: Mutex::new(store::Store::open(store::path())?),
            rules: launch::Rules::load()?,
            operations: Mutex::new(()),
            jobs: Mutex::new(BTreeMap::new()),
            baseline: Mutex::new(None),
        }))
    }
    async fn projd(&self) -> anyhow::Result<Proxy<'_>> {
        Ok(Proxy::new(
            &self.connection,
            goal_model::BUS_NAME,
            goal_model::ROOT_PATH,
            goal_model::MANAGER_INTERFACE,
        )
        .await?)
    }
}

pub struct Sessions(pub Arc<Manager>);

/// Remove live logical associations to deleted projects. Immutable saved
/// checkpoint references remain as provenance, so restoring one reports the
/// missing project instead of silently treating it as an unassigned workspace.
pub(crate) async fn reconcile_projects(
    locus: &LocusClient,
    valid: &std::collections::HashSet<String>,
) -> anyhow::Result<()> {
    for record in locus.list(links::SLOT_PROJECT).await? {
        if record
            .metadata
            .get("managed-by")
            .is_some_and(|v| v == "rsynapse-steward")
            && links::key(&record.target, links::PROJECT).is_some_and(|id| !valid.contains(&id))
        {
            locus
                .unset(record.subject, links::SLOT_PROJECT, record.target)
                .await?;
        }
    }
    Ok(())
}

fn failed(e: anyhow::Error) -> zbus::fdo::Error {
    zbus::fdo::Error::Failed(format!("{e:#}"))
}

#[interface(name = "org.rsynapse.Steward.Session1")]
impl Sessions {
    async fn list(&self) -> Vec<String> {
        self.0.store.lock().await.list()
    }
    async fn get(&self, name: String) -> zbus::fdo::Result<Session> {
        self.0.store.lock().await.get(&name).map_err(failed)
    }
    async fn save(&self, name: String) -> zbus::fdo::Result<Session> {
        if name.is_empty() || ["current", "previous", topology::CHECKPOINT].contains(&name.as_str())
        {
            return Err(zbus::fdo::Error::InvalidArgs(
                "choose last or a non-reserved session name".into(),
            ));
        }
        let _guard = self.0.operations.lock().await;
        self.0.capture(&name, true).await.map_err(failed)
    }
    async fn restore(
        &self,
        name: String,
        #[zbus(signal_context)] context: zbus::object_server::SignalContext<'_>,
    ) -> zbus::fdo::Result<String> {
        let session = self.0.store.lock().await.get(&name).map_err(failed)?;
        let identity = model::id();
        self.0.jobs.lock().await.insert(
            identity.clone(),
            RestoreReport {
                state: "running".into(),
                ..Default::default()
            },
        );
        let manager = self.0.clone();
        let job = identity.clone();
        let connection = context.connection().clone();
        tokio::spawn(async move {
            let _guard = manager.operations.lock().await;
            let mut report = match manager.restore(session).await {
                Ok(report) => report,
                Err(e) => RestoreReport {
                    issues: vec![format!("{e:#}")],
                    ..Default::default()
                },
            };
            report.state = if report.issues.is_empty() {
                "completed"
            } else {
                "partial"
            }
            .into();
            manager
                .jobs
                .lock()
                .await
                .insert(job.clone(), report.clone());
            let _ = connection
                .emit_signal(
                    None::<&str>,
                    ROOT,
                    INTERFACE,
                    "RestoreChanged",
                    &(job, report),
                )
                .await;
        });
        Ok(identity)
    }
    async fn get_restore(&self, job: String) -> zbus::fdo::Result<RestoreReport> {
        self.0
            .jobs
            .lock()
            .await
            .get(&job)
            .cloned()
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("unknown restore job".into()))
    }
    #[zbus(signal)]
    async fn restore_changed(
        context: &zbus::object_server::SignalContext<'_>,
        job: String,
        report: RestoreReport,
    ) -> zbus::Result<()>;
}

pub async fn watch(manager: Arc<Manager>) -> anyhow::Result<()> {
    let watch = Connection::session().await?;
    let mut source = observe::watch(&watch).await?;
    let mut relations = zbus::MessageStream::for_match_rule(
        zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(locus::BUS_NAME)?
            .build(),
        &watch,
        Some(256),
    )
    .await?;
    let bus = zbus::fdo::DBusProxy::new(&watch).await?;
    let mut owners = bus.receive_name_owner_changed().await?;
    loop {
        {
            let _guard = manager.operations.lock().await;
            if let Err(e) = manager.capture("current", false).await {
                eprintln!("[steward/session] capture deferred: {e:#}");
            }
        }
        loop {
            let changed = tokio::select! {
                message = source.next() => meaningful(&message.ok_or_else(|| anyhow::anyhow!("session source ended"))??),
                message = relations.next() => {
                    let message = message.ok_or_else(|| anyhow::anyhow!("session relation source ended"))??;
                    message.body().deserialize::<locus::RelationRecord>().is_ok_and(|r| r.relation == crate::relations::WORKSPACE_PROJECT)
                        || message.body().deserialize::<(locus::RelationEndpoint, String)>()
                            .is_ok_and(|(_, relation)| relation == crate::relations::WORKSPACE_PROJECT)
                },
                message = owners.next() => {
                    let message = message.ok_or_else(|| anyhow::anyhow!("session owner source ended"))?;
                    let args = message.args()?;
                    [workspace_dbus_api::BUS_NAME, locus::BUS_NAME, goal_model::BUS_NAME].contains(&args.name().as_str())
                }
            };
            if changed {
                break;
            }
        }
    }
}

fn meaningful(message: &zbus::Message) -> bool {
    if message
        .header()
        .member()
        .is_some_and(|m| m.as_str() != "PropertiesChanged")
    {
        return true;
    }
    let Ok((interface, changed, invalidated)) = message.body().deserialize::<(
        String,
        BTreeMap<String, zbus::zvariant::OwnedValue>,
        Vec<String>,
    )>() else {
        return true;
    };
    let keys: &[&str] = match interface.as_str() {
        workspace_dbus_api::MANAGER_INTERFACE => {
            &["Connected", "Generation", "Workspaces", "Windows"]
        }
        workspace_dbus_api::WORKSPACE_INTERFACE => {
            &["Name", "Index", "Layout", "Output", "Windows"]
        }
        workspace_dbus_api::WINDOW_INTERFACE => {
            &["Workspace", "AppId", "Floating", "ColumnIndex", "RowIndex"]
        }
        workspace_dbus_api::OUTPUT_INTERFACE => &["Enabled"],
        _ => &[],
    };
    keys.iter()
        .any(|key| changed.contains_key(*key) || invalidated.iter().any(|v| v == key))
}
