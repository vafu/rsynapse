//! Session integration policy. Projd remains unaware of these desktop concepts.
use crate::relations::{self, LocusClient};
use data_sources::rx::Observable as _;
use futures_util::StreamExt;
use locus::RelationEndpoint;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};
use zbus::{Connection, Proxy, interface};

const ROOT: &str = "/org/rsynapse/Steward";
mod icons;
mod removal;
mod workspace_cleanup;
const BUS: &str = "org.rsynapse.Steward";
const WINDOW_PROJECT: &str = "org.rsynapse.window.project";
const PROJECT_AGENT: &str = "org.rsynapse.project.agent";
static BIND_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
fn owned() -> HashMap<String, String> {
    HashMap::from([("managed-by".into(), "rsynapse-steward".into())])
}
async fn proj() -> anyhow::Result<Proxy<'static>> {
    let conn = Connection::session().await?;
    Ok(Proxy::new_owned(
        conn,
        proj_model::BUS_NAME,
        proj_model::ROOT_PATH,
        proj_model::MANAGER_INTERFACE,
    )
    .await?)
}

async fn bind_workspace(
    locus: &LocusClient,
    path: &str,
    workspace: String,
) -> anyhow::Result<proj_model::ProjectInfo> {
    let _lock = BIND_LOCK.lock().await;
    workspace_cleanup::synchronize(locus).await?;
    let context = proj().await?.call("RegisterProject", &(path,)).await?;
    bind_context(locus, context, Some(workspace), None).await
}

/// An observed CWD attributes the window, independently of the workspace's
/// explicit project selection. It never creates a project or a workspace bind.
async fn bind_window(
    locus: &LocusClient,
    path: &str,
    window: String,
) -> anyhow::Result<proj_model::ProjectInfo> {
    let _lock = BIND_LOCK.lock().await;
    let context = proj().await?.call("ResolveProject", &(path,)).await?;
    bind_context(locus, context, None, Some(window)).await
}
async fn bind_context(
    locus: &LocusClient,
    context: proj_model::ProjectInfo,
    workspace: Option<String>,
    window: Option<String>,
) -> anyhow::Result<proj_model::ProjectInfo> {
    let proxy = proj().await?;
    let checkouts: Vec<proj_model::CheckoutInfo> = proxy.call("ListCheckouts", &()).await?;
    let checkout = checkouts.iter().find(|c| c.id == context.checkout_id);
    let target = RelationEndpoint::stable_key(
        locus::keys::PROJECT_PATH,
        checkout
            .map(|c| c.root_path.as_str())
            .unwrap_or(&context.cwd),
    );
    let mut metadata = owned();
    metadata.insert("project-id".into(), context.id.clone());
    metadata.insert("checkout-id".into(), context.checkout_id.clone());
    if let Some(workspace) = workspace {
        workspace_cleanup::stamp(&mut metadata, workspace_cleanup::synchronize(locus).await?);
        require_workspace(&workspace).await?;
        locus
            .set_one_with_persistence(
                RelationEndpoint::stable_key(locus::keys::WORKSPACE_ID, workspace.clone()),
                relations::WORKSPACE_PROJECT,
                target.clone(),
                metadata.clone(),
                false,
            )
            .await?;
    }
    if let Some(window) = window {
        locus
            .set_one_with_persistence(
                RelationEndpoint::stable_key(locus::keys::WINDOW_ID, window),
                WINDOW_PROJECT,
                target,
                metadata,
                false,
            )
            .await?;
    }
    Ok(context)
}
pub async fn bind_current(
    locus: &LocusClient,
    path: &str,
) -> anyhow::Result<proj_model::ProjectInfo> {
    bind_workspace(locus, path, focused_workspace().await?).await
}
pub(crate) async fn restore_workspace(
    locus: &LocusClient,
    workspace: &str,
    project: Option<proj_model::ProjectInfo>,
) -> anyhow::Result<()> {
    if let Some(project) = project {
        let _guard = BIND_LOCK.lock().await;
        workspace_cleanup::synchronize(locus).await?;
        bind_context(locus, project, Some(workspace.into()), None).await?;
    } else {
        Bindings {
            locus: locus.clone(),
        }
        .unassign_workspace_project(workspace.into())
        .await?;
    }
    Ok(())
}
async fn focused_workspace() -> anyhow::Result<String> {
    let conn = Connection::session().await?;
    let manager = workspace_dbus_api::ManagerProxy::new(&conn).await?;
    let paths = manager.focused_workspace().await?;
    let path = paths
        .first()
        .ok_or_else(|| anyhow::anyhow!("No focused workspace"))?;
    Ok(workspace_dbus_api::WorkspaceProxy::builder(&conn)
        .path(path.clone())?
        .build()
        .await?
        .id()
        .await?)
}
async fn require_workspace(id: &str) -> anyhow::Result<()> {
    let conn = Connection::session().await?;
    let manager = workspace_dbus_api::ManagerProxy::new(&conn).await?;
    anyhow::ensure!(
        manager.connected().await?
            && manager
                .workspaces()
                .await?
                .contains(&workspace_dbus_api::workspace_path(id)),
        "Workspace no longer exists"
    );
    Ok(())
}
#[derive(Clone)]
struct Bindings {
    locus: LocusClient,
}
#[interface(name = "org.rsynapse.Steward.Associations1")]
impl Bindings {
    async fn unassign_workspace_project(&self, workspace: String) -> zbus::fdo::Result<()> {
        let result = async {
            let _lock = BIND_LOCK.lock().await;
            let subject = RelationEndpoint::stable_key(locus::keys::WORKSPACE_ID, workspace);
            for record in self.locus.list(relations::WORKSPACE_PROJECT).await? {
                if record.subject == subject {
                    self.locus
                        .unset(record.subject, relations::WORKSPACE_PROJECT, record.target)
                        .await?;
                }
            }
            Ok::<_, anyhow::Error>(())
        }
        .await;
        result.map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
    async fn bind_workspace_project(
        &self,
        workspace: String,
        path: String,
    ) -> zbus::fdo::Result<proj_model::ProjectInfo> {
        bind_workspace(&self.locus, &path, workspace)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
    async fn get_init_workspace(&self) -> zbus::fdo::Result<String> {
        let workspace = focused_workspace()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        let subject = RelationEndpoint::stable_key(locus::keys::WORKSPACE_ID, workspace.clone());
        if self
            .locus
            .list(relations::WORKSPACE_PROJECT)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?
            .iter()
            .any(|r| r.subject == subject)
        {
            return Err(zbus::fdo::Error::InvalidArgs(
                "Workspace already has a project".into(),
            ));
        }
        Ok(workspace)
    }
    async fn init_workspace_project(
        &self,
        workspace: String,
        path: String,
    ) -> zbus::fdo::Result<proj_model::ProjectInfo> {
        let _lock = BIND_LOCK.lock().await;
        workspace_cleanup::synchronize(&self.locus)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        let subject = RelationEndpoint::stable_key(locus::keys::WORKSPACE_ID, workspace.clone());
        let existing = self
            .locus
            .list(relations::WORKSPACE_PROJECT)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        if existing.iter().any(|r| r.subject == subject) {
            return Err(zbus::fdo::Error::InvalidArgs(
                "Workspace already has a project".into(),
            ));
        }
        // Resolve the directory before checking the binding again, so a concurrent
        // association during a slow project scan is never silently replaced.
        let proxy = proj()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        let context: proj_model::ProjectInfo =
            proxy.call("RegisterProject", &(path.clone(),)).await?;
        let existing = self
            .locus
            .list(relations::WORKSPACE_PROJECT)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        if existing.iter().any(|r| r.subject == subject) {
            return Err(zbus::fdo::Error::InvalidArgs(
                "Workspace already has a project".into(),
            ));
        }
        bind_context(&self.locus, context, Some(workspace), None)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
    async fn bind_current_project(
        &self,
        path: String,
    ) -> zbus::fdo::Result<proj_model::ProjectInfo> {
        bind_current(&self.locus, &path)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
}
pub async fn run(locus: LocusClient) -> anyhow::Result<()> {
    // Poll independently: cleanup and binding both hold BIND_LOCK across D-Bus
    // calls. An inline select future could hold the lock while a selected
    // binding branch waits for it, preventing that future from being polled.
    let mut cleanup = tokio::task::JoinSet::new();
    let session = crate::session::Manager::new(locus.clone()).await?;
    cleanup.spawn(workspace_cleanup::run(locus.clone()));
    let _conn = zbus::connection::Builder::session()?
        .serve_at(ROOT, crate::session::Sessions(session.clone()))?
        .serve_at(
            ROOT,
            Bindings {
                locus: locus.clone(),
            },
        )?
        .name(BUS)?
        .build()
        .await?;
    cleanup.spawn(crate::session::watch(session));
    let mut agents = crate::agent_metrics::snapshots(&locus).into_stream();
    let mut checkouts = data_sources::proj::checkouts().into_stream();
    let mut projects = data_sources::proj::projects().into_stream();
    let mut removed = data_sources::proj::removed_projects().into_stream();
    let mut known = Vec::<proj_model::CheckoutInfo>::new();
    let mut known_projects = Vec::<proj_model::ProjectInfo>::new();
    let mut seen = HashSet::new();
    let mut agent_active = true;
    loop {
        tokio::select! {
            result = cleanup.join_next() => return result.ok_or_else(|| anyhow::anyhow!("Workspace cleanup task ended"))??,
            item=checkouts.next()=>match item{Some(Ok(rows))=>known=rows,Some(Err(e))=>return Err(anyhow::anyhow!(e)),None=>return Err(anyhow::anyhow!("Checkout source ended"))},
            item=projects.next()=>match item{Some(Ok(rows))=>{known_projects=rows.clone();removal::reconcile(&locus,rows).await?;},Some(Err(e))=>return Err(anyhow::anyhow!(e)),None=>return Err(anyhow::anyhow!("Project source ended"))},
            item=removed.next()=>match item{Some(Ok(info))=>removal::removed(&locus,info).await?,Some(Err(e))=>return Err(anyhow::anyhow!(e)),None=>return Err(anyhow::anyhow!("Project removal source ended"))},
            item=agents.next(),if agent_active=>if let Some(Ok(snapshot))=item {
                for session in &snapshot.sessions {
                    if session.subagent||session.cwd.is_empty()||session.window.is_none(){continue;}
                    let identity=(session.key.clone(),session.cwd.clone(),session.window.clone());
                    if seen.contains(&identity){continue;}
                    if !known.iter().any(|c|Path::new(&session.cwd).starts_with(&c.root_path)) && !known_projects.iter().any(|p|p.checkout_id.is_empty() && Path::new(&session.cwd).starts_with(&p.cwd)){continue;}
                    let context=match bind_window(&locus,&session.cwd,session.window.clone().unwrap()).await { Ok(context)=>context,Err(error)=>{eprintln!("[steward/associations] {}: {error}",session.cwd);continue;} };
                    let agent=RelationEndpoint::stable_key(locus::keys::AGENT_SESSION_ID,format!("{}/{}",session.agent,session.session_id));
                    let project=RelationEndpoint::dbus_object("session",proj_model::BUS_NAME,proj_model::object_path("Projects",&context.id),proj_model::PROJECT_INTERFACE);
                    locus.set_with_persistence(project,PROJECT_AGENT,agent.clone(),owned(),false).await?;
                    if let Some(window)=&session.window {
                        if snapshot.active_links.get(window).is_none_or(|key|key==&session.key){let mut metadata=owned();metadata.insert("session_path".into(),session.key.clone());locus.set_one_with_persistence(RelationEndpoint::stable_key(locus::keys::WINDOW_ID,window.clone()),relations::WINDOW_AGENT_SESSION,agent,metadata,false).await?;}
                    }
                    seen.insert(identity);
                }
                let live:HashSet<_>=snapshot.sessions.iter().map(|s|s.key.clone()).collect();seen.retain(|(key,_,_)|live.contains(key));
                let agents:HashSet<_>=snapshot.sessions.iter().map(|s|format!("{}/{}",s.agent,s.session_id)).collect();
                for r in locus.list(PROJECT_AGENT).await? {if r.metadata.get("managed-by").is_some_and(|v|v=="rsynapse-steward") {if let RelationEndpoint::StableKey{kind,id}=&r.target {if kind==locus::keys::AGENT_SESSION_ID&&!agents.contains(id){locus.unset(r.subject,PROJECT_AGENT,r.target).await?;}}}}
            }else{agent_active=false;}
        }
    }
}

#[cfg(test)]
mod tests;
