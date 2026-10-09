//! Session integration policy. Projd remains unaware of these desktop concepts.
use crate::relations::{self, LocusClient};
use futures_util::StreamExt;
use locus::RelationEndpoint;
use shell_source::rx::Observable as _;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};
use zbus::{Connection, Proxy, interface};

const ROOT: &str = "/org/rsynapse/Steward";
mod icons;
mod removal;
const BUS: &str = "org.rsynapse.Steward";
const PROJECT_GOAL: &str = "org.rsynapse.goal.project";
const WINDOW_PROJECT: &str = "org.rsynapse.window.project";
const PROJECT_AGENT: &str = "org.rsynapse.project.agent";
const WORKSPACE_PROJECT_POLICY: &str = "org.rsynapse.workspace.project-policy";
static BIND_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
fn owned() -> HashMap<String, String> {
    HashMap::from([("managed-by".into(), "rsynapse-steward".into())])
}
async fn proj() -> anyhow::Result<Proxy<'static>> {
    let conn = Connection::session().await?;
    Ok(Proxy::new_owned(
        conn,
        goal_model::BUS_NAME,
        goal_model::ROOT_PATH,
        goal_model::MANAGER_INTERFACE,
    )
    .await?)
}

async fn bind(
    locus: &LocusClient,
    path: &str,
    mut workspace: Option<u64>,
    window: Option<u64>,
    automatic: bool,
) -> anyhow::Result<goal_model::ProjectInfo> {
    let _lock = BIND_LOCK.lock().await;
    if automatic {
        if let Some(id) = workspace {
            let subject =
                RelationEndpoint::stable_key(locus::keys::NIRI_WORKSPACE_ID, id.to_string());
            let existing = locus.list(relations::WORKSPACE_PROJECT).await?;
            let policies = locus.list(WORKSPACE_PROJECT_POLICY).await?;
            if existing.iter().any(|r| r.subject == subject)
                || policies.iter().any(|r| {
                    r.subject == subject
                        && r.metadata.get("automatic").is_some_and(|v| v == "false")
                })
            {
                workspace = None;
            }
        }
    }
    let proxy = proj().await?;
    let context: goal_model::ProjectInfo = proxy
        .call(
            if automatic {
                "ResolveProject"
            } else {
                "RegisterProject"
            },
            &(path,),
        )
        .await?;
    bind_context(locus, context, workspace, window, !automatic).await
}
async fn bind_context(
    locus: &LocusClient,
    context: goal_model::ProjectInfo,
    workspace: Option<u64>,
    window: Option<u64>,
    explicit: bool,
) -> anyhow::Result<goal_model::ProjectInfo> {
    let proxy = proj().await?;
    let checkouts: Vec<goal_model::CheckoutInfo> = proxy.call("ListCheckouts", &()).await?;
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
        locus
            .set_one_with_persistence(
                RelationEndpoint::stable_key(locus::keys::NIRI_WORKSPACE_ID, workspace.to_string()),
                relations::WORKSPACE_PROJECT,
                target.clone(),
                metadata.clone(),
                false,
            )
            .await?;
        if explicit {
            let subject =
                RelationEndpoint::stable_key(locus::keys::NIRI_WORKSPACE_ID, workspace.to_string());
            for policy in locus.list(WORKSPACE_PROJECT_POLICY).await? {
                if policy.subject == subject {
                    locus
                        .unset(policy.subject, WORKSPACE_PROJECT_POLICY, policy.target)
                        .await?;
                }
            }
        }
    }
    if let Some(window) = window {
        locus
            .set_one_with_persistence(
                RelationEndpoint::stable_key(locus::keys::NIRI_WINDOW_ID, window.to_string()),
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
) -> anyhow::Result<goal_model::ProjectInfo> {
    bind(locus, path, Some(focused_workspace().await?), None, false).await
}
async fn focused_workspace() -> anyhow::Result<u64> {
    let conn = Connection::session().await?;
    let niri = Proxy::new(
        &conn,
        niri_dbus::BUS_NAME,
        niri_dbus::ROOT_PATH,
        "org.rsynapse.Niri1",
    )
    .await?;
    let workspace: Vec<zbus::zvariant::OwnedObjectPath> =
        niri.get_property("FocusedWorkspace").await?;
    let workspace = workspace
        .first()
        .and_then(|p| p.as_str().rsplit("workspace_").next()?.parse().ok());
    workspace.ok_or_else(|| anyhow::anyhow!("No focused workspace"))
}
#[derive(Clone)]
struct Bindings {
    locus: LocusClient,
}
#[interface(name = "org.rsynapse.Steward.Associations1")]
impl Bindings {
    async fn unassign_workspace_project(&self, workspace: u64) -> zbus::fdo::Result<()> {
        let result = async {
            let _lock = BIND_LOCK.lock().await;
            let subject =
                RelationEndpoint::stable_key(locus::keys::NIRI_WORKSPACE_ID, workspace.to_string());
            let mut metadata = owned();
            metadata.insert("automatic".into(), "false".into());
            self.locus
                .set_one_with_persistence(
                    subject.clone(),
                    WORKSPACE_PROJECT_POLICY,
                    RelationEndpoint::stable_key("org.rsynapse.project-assignment", "unassigned"),
                    metadata,
                    false,
                )
                .await?;
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
        workspace: u64,
        path: String,
    ) -> zbus::fdo::Result<goal_model::ProjectInfo> {
        bind(&self.locus, &path, Some(workspace), None, false)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
    async fn get_init_workspace(&self) -> zbus::fdo::Result<u64> {
        let workspace = focused_workspace()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        let subject =
            RelationEndpoint::stable_key(locus::keys::NIRI_WORKSPACE_ID, workspace.to_string());
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
        workspace: u64,
        path: String,
    ) -> zbus::fdo::Result<goal_model::ProjectInfo> {
        let _lock = BIND_LOCK.lock().await;
        let subject =
            RelationEndpoint::stable_key(locus::keys::NIRI_WORKSPACE_ID, workspace.to_string());
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
        let context: goal_model::ProjectInfo =
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
        bind_context(&self.locus, context, Some(workspace), None, true)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
    async fn clear_goal_sessions(&self, key: String) -> zbus::fdo::Result<()> {
        let target = RelationEndpoint::stable_key(goal_model::GOAL_KIND, key);
        for r in self
            .locus
            .list(goal_model::AGENT_GOAL)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?
        {
            if r.target == target {
                self.locus
                    .unset(r.subject, goal_model::AGENT_GOAL, r.target)
                    .await
                    .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
            }
        }
        Ok(())
    }
    async fn bind_current_project(
        &self,
        path: String,
    ) -> zbus::fdo::Result<goal_model::ProjectInfo> {
        bind_current(&self.locus, &path)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
    async fn bind_agent_goal(
        &self,
        key: String,
        agent: String,
        session: String,
    ) -> zbus::fdo::Result<()> {
        let proxy = proj()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        let goals: Vec<goal_model::GoalInfo> = proxy.call("ListGoals", &(String::new(),)).await?;
        if !goals
            .iter()
            .any(|g| format!("{}/{}", g.date, g.id) == key && g.kind == "outcome")
        {
            return Err(zbus::fdo::Error::InvalidArgs(
                "An outcome goal is required".into(),
            ));
        }
        let agent = crate::focus::canonical_app_id(&agent);
        self.locus
            .set_with_persistence(
                RelationEndpoint::stable_key(
                    locus::keys::AGENT_SESSION_ID,
                    format!("{agent}/{session}"),
                ),
                goal_model::AGENT_GOAL,
                RelationEndpoint::stable_key(goal_model::GOAL_KIND, key),
                owned(),
                true,
            )
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
    async fn unbind_agent_goal(
        &self,
        key: String,
        agent: String,
        session: String,
    ) -> zbus::fdo::Result<()> {
        let agent = crate::focus::canonical_app_id(&agent);
        self.locus
            .unset(
                RelationEndpoint::stable_key(
                    locus::keys::AGENT_SESSION_ID,
                    format!("{agent}/{session}"),
                ),
                goal_model::AGENT_GOAL,
                RelationEndpoint::stable_key(goal_model::GOAL_KIND, key),
            )
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
}
async fn goal_links(locus: &LocusClient, goals: Vec<goal_model::GoalInfo>) -> anyhow::Result<()> {
    let keys: HashSet<_> = goals
        .iter()
        .map(|g| format!("{}/{}", g.date, g.id))
        .collect();
    for g in goals {
        let target =
            RelationEndpoint::stable_key(goal_model::GOAL_KIND, format!("{}/{}", g.date, g.id));
        locus
            .set_with_persistence(
                RelationEndpoint::stable_key(goal_model::DAY_KIND, &g.date),
                goal_model::DAY_GOAL,
                target.clone(),
                owned(),
                true,
            )
            .await?;
        let existing = locus.list(PROJECT_GOAL).await?;
        for r in existing.into_iter().filter(|r| {
            r.subject == target
                && r.metadata
                    .get("managed-by")
                    .is_some_and(|v| v == "rsynapse-steward")
        }) {
            if r.target != RelationEndpoint::stable_key(locus::keys::PROJECT_PATH, &g.project) {
                locus.unset(r.subject, PROJECT_GOAL, r.target).await?;
            }
        }
        if !g.project.is_empty() {
            locus
                .set_with_persistence(
                    target,
                    PROJECT_GOAL,
                    RelationEndpoint::stable_key(locus::keys::PROJECT_PATH, g.project),
                    owned(),
                    true,
                )
                .await?;
        }
    }
    for relation in [goal_model::DAY_GOAL, PROJECT_GOAL, goal_model::AGENT_GOAL] {
        for r in locus.list(relation).await? {
            if r.metadata
                .get("managed-by")
                .is_none_or(|v| v != "rsynapse-steward")
            {
                continue;
            }
            let goal = if relation == PROJECT_GOAL {
                &r.subject
            } else {
                &r.target
            };
            if let RelationEndpoint::StableKey { kind, id } = goal {
                if kind == goal_model::GOAL_KIND && !keys.contains(id) {
                    locus.unset(r.subject, relation, r.target).await?;
                }
            }
        }
    }
    Ok(())
}
pub async fn run(locus: LocusClient) -> anyhow::Result<()> {
    let _conn = zbus::connection::Builder::session()?
        .serve_at(
            ROOT,
            Bindings {
                locus: locus.clone(),
            },
        )?
        .name(BUS)?
        .build()
        .await?;
    let mut goals = shell_source::proj::goals().into_stream();
    let mut agents = crate::agent_metrics::snapshots(&locus).into_stream();
    let mut checkouts = shell_source::proj::checkouts().into_stream();
    let mut projects = shell_source::proj::projects().into_stream();
    let mut removed = shell_source::proj::removed_projects().into_stream();
    let mut known = Vec::<goal_model::CheckoutInfo>::new();
    let mut known_projects = Vec::<goal_model::ProjectInfo>::new();
    let mut seen = HashSet::new();
    let mut agent_active = true;
    loop {
        tokio::select! {
            item=goals.next()=>match item{Some(Ok(goals))=>goal_links(&locus,goals).await?,Some(Err(e))=>return Err(anyhow::anyhow!(e)),None=>return Err(anyhow::anyhow!("Goal source ended"))},
            item=checkouts.next()=>match item{Some(Ok(rows))=>known=rows,Some(Err(e))=>return Err(anyhow::anyhow!(e)),None=>return Err(anyhow::anyhow!("Checkout source ended"))},
            item=projects.next()=>match item{Some(Ok(rows))=>{known_projects=rows.clone();removal::reconcile(&locus,rows).await?;},Some(Err(e))=>return Err(anyhow::anyhow!(e)),None=>return Err(anyhow::anyhow!("Project source ended"))},
            item=removed.next()=>match item{Some(Ok(info))=>removal::removed(&locus,info).await?,Some(Err(e))=>return Err(anyhow::anyhow!(e)),None=>return Err(anyhow::anyhow!("Project removal source ended"))},
            item=agents.next(),if agent_active=>if let Some(Ok(snapshot))=item {
                for session in &snapshot.sessions {
                    if session.subagent||session.cwd.is_empty()||session.window.is_none(){continue;}
                    let identity=(session.key.clone(),session.cwd.clone(),session.window,session.workspace_id);
                    if seen.contains(&identity){continue;}
                    if !known.iter().any(|c|Path::new(&session.cwd).starts_with(&c.root_path)) && !known_projects.iter().any(|p|p.checkout_id.is_empty() && Path::new(&session.cwd).starts_with(&p.cwd)){continue;}
                    let mut workspace=session.workspace_id;
                    if let Some(id)=workspace {
                        let existing=locus.list(relations::WORKSPACE_PROJECT).await?;
                        // Replay must preserve an existing binding's context and persistence.
                        if existing.iter().any(|r|r.subject==RelationEndpoint::stable_key(locus::keys::NIRI_WORKSPACE_ID,id.to_string())){workspace=None;}
                    }
                    let context=match bind(&locus,&session.cwd,workspace,session.window,true).await { Ok(context)=>context,Err(error)=>{eprintln!("[steward/associations] {}: {error}",session.cwd);continue;} };
                    let agent=RelationEndpoint::stable_key(locus::keys::AGENT_SESSION_ID,format!("{}/{}",session.agent,session.session_id));
                    let project=RelationEndpoint::dbus_object("session",goal_model::BUS_NAME,goal_model::object_path("Projects",&context.id),goal_model::PROJECT_INTERFACE);
                    locus.set_with_persistence(project,PROJECT_AGENT,agent.clone(),owned(),false).await?;
                    if let Some(window)=session.window {
                        if snapshot.active_links.get(&window).is_none_or(|key|key==&session.key){let mut metadata=owned();metadata.insert("session_path".into(),session.key.clone());locus.set_one_with_persistence(RelationEndpoint::stable_key(locus::keys::NIRI_WINDOW_ID,window.to_string()),relations::WINDOW_AGENT_SESSION,agent,metadata,false).await?;}
                    }
                    seen.insert(identity);
                }
                let live:HashSet<_>=snapshot.sessions.iter().map(|s|s.key.clone()).collect();seen.retain(|(key,_,_,_)|live.contains(key));
                let agents:HashSet<_>=snapshot.sessions.iter().map(|s|format!("{}/{}",s.agent,s.session_id)).collect();
                for r in locus.list(PROJECT_AGENT).await? {if r.metadata.get("managed-by").is_some_and(|v|v=="rsynapse-steward") {if let RelationEndpoint::StableKey{kind,id}=&r.target {if kind==locus::keys::AGENT_SESSION_ID&&!agents.contains(id){locus.unset(r.subject,PROJECT_AGENT,r.target).await?;}}}}
            }else{agent_active=false;}
        }
    }
}
