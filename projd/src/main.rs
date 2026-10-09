mod git;
mod projects;
mod store;
mod watch;
use proj_model::*;
use std::{path::Path, sync::Arc};
use store::Store;
use tokio::sync::Mutex;
use zbus::{Connection, interface, object_server::SignalContext, zvariant::OwnedObjectPath};

#[derive(Clone)]
struct Manager {
    store: Arc<Store>,
    mutation: Arc<Mutex<()>>,
    watch: tokio::sync::mpsc::UnboundedSender<watch::Request>,
    checkout_mutations: Arc<Mutex<std::collections::HashMap<String, Arc<Mutex<()>>>>>,
}
fn error(e: impl std::fmt::Display) -> zbus::fdo::Error {
    zbus::fdo::Error::Failed(e.to_string())
}
fn path(kind: &str, id: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(object_path(kind, id)).unwrap()
}
fn id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

struct ProjectObject(ProjectInfo);
#[interface(name = "org.rsynapse.Proj.Project1")]
impl ProjectObject {
    #[zbus(property)]
    fn id(&self) -> &str {
        &self.0.id
    }
    #[zbus(property)]
    fn name(&self) -> &str {
        &self.0.name
    }
    #[zbus(property)]
    fn root_path(&self) -> &str {
        &self.0.root_path
    }
}
struct CheckoutObject(CheckoutInfo);
#[interface(name = "org.rsynapse.Proj.Checkout1")]
impl CheckoutObject {
    #[zbus(property)]
    fn id(&self) -> &str {
        &self.0.id
    }
    #[zbus(property)]
    fn project_id(&self) -> &str {
        &self.0.project_id
    }
    #[zbus(property)]
    fn root_path(&self) -> &str {
        &self.0.root_path
    }
    #[zbus(property)]
    fn branch(&self) -> &str {
        &self.0.branch
    }
    #[zbus(property)]
    fn git_status(&self) -> (u32, u32, u32, u32, u32, bool, bool, u32) {
        let s = &self.0.git_status;
        (
            s.staged,
            s.unstaged,
            s.untracked,
            s.ahead,
            s.behind,
            s.merging,
            s.rebasing,
            s.stashes,
        )
    }
}
struct ContextObject(ContextInfo);
#[interface(name = "org.rsynapse.Proj.Context1")]
impl ContextObject {
    #[zbus(property)]
    fn id(&self) -> &str {
        &self.0.id
    }
    #[zbus(property)]
    fn project_id(&self) -> &str {
        &self.0.project_id
    }
    #[zbus(property)]
    fn checkout_id(&self) -> &str {
        &self.0.checkout_id
    }
    #[zbus(property)]
    fn cwd(&self) -> &str {
        &self.0.cwd
    }
    #[zbus(property)]
    fn relative_cwd(&self) -> &str {
        &self.0.relative_cwd
    }
}
struct GoalObject(GoalInfo);
#[interface(name = "org.rsynapse.Proj.Goal1")]
impl GoalObject {
    #[zbus(property)]
    fn id(&self) -> &str {
        &self.0.id
    }
    #[zbus(property)]
    fn date(&self) -> &str {
        &self.0.date
    }
    #[zbus(property)]
    fn title(&self) -> &str {
        &self.0.title
    }
    #[zbus(property)]
    fn kind(&self) -> &str {
        &self.0.kind
    }
    #[zbus(property)]
    fn project(&self) -> &str {
        &self.0.project
    }
    #[zbus(property)]
    fn success_criterion(&self) -> &str {
        &self.0.success
    }
    #[zbus(property)]
    fn priority(&self) -> &str {
        &self.0.priority
    }
    #[zbus(property)]
    fn status(&self) -> &str {
        &self.0.status
    }
}

async fn publish_project(conn: &Connection, info: ProjectInfo) -> zbus::Result<()> {
    let p = path("Projects", &info.id);
    if let Ok(handle) = conn.object_server().interface::<_, ProjectObject>(&p).await {
        let before = {
            let mut object = handle.get_mut().await;
            let old = object.0.clone();
            object.0 = info.clone();
            old
        };
        let object = handle.get().await;
        if before.name != info.name {
            object.name_changed(handle.signal_context()).await?;
        }
        if before.root_path != info.root_path {
            object.root_path_changed(handle.signal_context()).await?;
        }
    } else {
        conn.object_server().at(p, ProjectObject(info)).await?;
    }
    Ok(())
}
async fn publish_checkout(conn: &Connection, info: CheckoutInfo) -> zbus::Result<()> {
    let p = path("Checkouts", &info.id);
    if let Ok(handle) = conn
        .object_server()
        .interface::<_, CheckoutObject>(&p)
        .await
    {
        let before = {
            let mut object = handle.get_mut().await;
            let old = object.0.clone();
            object.0 = info.clone();
            old
        };
        let object = handle.get().await;
        if before.branch != info.branch {
            object.branch_changed(handle.signal_context()).await?;
        }
        if before.git_status != info.git_status {
            object.git_status_changed(handle.signal_context()).await?;
        }
    } else {
        conn.object_server().at(p, CheckoutObject(info)).await?;
    }
    Ok(())
}
async fn publish_goal(conn: &Connection, info: GoalInfo) -> zbus::Result<()> {
    let p = OwnedObjectPath::try_from(goal_path(&info.date, &info.id)).unwrap();
    if let Ok(handle) = conn.object_server().interface::<_, GoalObject>(&p).await {
        let before = {
            let mut object = handle.get_mut().await;
            let old = object.0.clone();
            object.0 = info.clone();
            old
        };
        let object = handle.get().await;
        let e = handle.signal_context();
        if before.title != info.title {
            object.title_changed(e).await?;
        }
        if before.kind != info.kind {
            object.kind_changed(e).await?;
        }
        if before.project != info.project {
            object.project_changed(e).await?;
        }
        if before.success != info.success {
            object.success_criterion_changed(e).await?;
        }
        if before.priority != info.priority {
            object.priority_changed(e).await?;
        }
        if before.status != info.status {
            object.status_changed(e).await?;
        }
    } else {
        conn.object_server().at(p, GoalObject(info)).await?;
    }
    Ok(())
}

impl Manager {
    async fn register(&self, cwd: String, conn: &Connection) -> zbus::fdo::Result<ContextInfo> {
        self.register_inner(cwd, conn, true, false).await
    }
    async fn register_inner(
        &self,
        cwd: String,
        conn: &Connection,
        subscribe: bool,
        existing_only: bool,
    ) -> zbus::fdo::Result<ContextInfo> {
        let (primary, root, cwd) =
            tokio::task::spawn_blocking(move || git::discover(Path::new(&cwd)))
                .await
                .map_err(error)?
                .map_err(error)?;
        let primary = primary.to_string_lossy().into_owned();
        let root = root.to_string_lossy().into_owned();
        if existing_only
            && !self
                .store
                .checkouts()
                .map_err(error)?
                .iter()
                .any(|c| c.root_path == root)
        {
            return Err(zbus::fdo::Error::FileNotFound(
                "Project is not registered; use proj add first".into(),
            ));
        }
        let project = self
            .store
            .projects()
            .map_err(error)?
            .into_iter()
            .find(|p| p.root_path == primary)
            .unwrap_or_else(|| ProjectInfo {
                id: id(),
                name: Path::new(&primary)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                root_path: primary,
            });
        self.store
            .put("project", &project.id, &project)
            .map_err(error)?;
        publish_project(conn, project.clone())
            .await
            .map_err(error)?;
        let existing = self
            .store
            .checkouts()
            .map_err(error)?
            .into_iter()
            .find(|c| c.root_path == root);
        let fresh = existing.is_none();
        let mut checkout = existing.unwrap_or_else(|| CheckoutInfo {
            id: id(),
            project_id: project.id.clone(),
            root_path: root.clone(),
            branch: String::new(),
            git_status: GitStatus::default(),
        });
        let owned = root.clone();
        let branch = tokio::task::spawn_blocking(move || git::branch(Path::new(&owned)))
            .await
            .map_err(error)?;
        checkout.branch = branch;
        self.store
            .put("checkout", &checkout.id, &checkout)
            .map_err(error)?;
        publish_checkout(conn, checkout.clone())
            .await
            .map_err(error)?;
        let cwd = cwd.to_string_lossy().into_owned();
        let context = self
            .store
            .contexts()
            .map_err(error)?
            .into_iter()
            .find(|c| c.cwd == cwd)
            .unwrap_or_else(|| ContextInfo {
                id: id(),
                project_id: project.id,
                checkout_id: checkout.id,
                cwd: cwd.clone(),
                relative_cwd: Path::new(&cwd)
                    .strip_prefix(&root)
                    .unwrap_or(Path::new(""))
                    .to_string_lossy()
                    .into_owned(),
            });
        self.store
            .put("context", &context.id, &context)
            .map_err(error)?;
        let p = path("Contexts", &context.id);
        conn.object_server()
            .at(p, ContextObject(context.clone()))
            .await
            .map_err(error)?;
        if let Some(dir) = git::git_dir(Path::new(&root)) {
            let (done, ready) = tokio::sync::oneshot::channel();
            let _ = self.watch.send((root, dir, fresh && subscribe, done));
            let _ = ready.await;
        }
        Ok(context)
    }

    async fn refresh_checkout(&self, root: String, conn: &Connection) -> zbus::fdo::Result<()> {
        if !self
            .store
            .checkouts()
            .map_err(error)?
            .iter()
            .any(|c| c.root_path == root)
        {
            return Ok(());
        }
        let lock = self
            .checkout_mutations
            .lock()
            .await
            .entry(root.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _checkout_lock = lock.lock().await;
        let owned = root.clone();
        let (branch, status) =
            tokio::task::spawn_blocking(move || git::snapshot(Path::new(&owned)))
                .await
                .map_err(error)?;
        // Expensive Git work must never hold the domain-wide mutation lock.
        let _lock = self.mutation.lock().await;
        if let Some(mut checkout) = self
            .store
            .checkouts()
            .map_err(error)?
            .into_iter()
            .find(|c| c.root_path == root)
        {
            checkout.branch = branch;
            checkout.git_status = status;
            self.store
                .put("checkout", &checkout.id, &checkout)
                .map_err(error)?;
            publish_checkout(conn, checkout.clone())
                .await
                .map_err(error)?;
            let e = SignalContext::new(conn, ROOT_PATH).map_err(error)?;
            Self::changed(&e, "checkout", &checkout.id)
                .await
                .map_err(error)?;
        }
        Ok(())
    }
}
#[interface(name = "org.rsynapse.Proj.Manager1")]
impl Manager {
    async fn remove_project(
        &self,
        id: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_context)] e: SignalContext<'_>,
    ) -> zbus::fdo::Result<bool> {
        let Some(removed) = self.delete_project(&id, conn).await? else {
            return Ok(false);
        };
        Self::project_removed(&e, removed).await.map_err(error)?;
        Self::changed(&e, "project-remove", &id)
            .await
            .map_err(error)?;
        Ok(true)
    }
    async fn resolve_context(
        &self,
        cwd: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_context)] e: SignalContext<'_>,
    ) -> zbus::fdo::Result<ContextInfo> {
        let _lock = self.mutation.lock().await;
        let context = self.register_inner(cwd, conn, true, true).await?;
        Self::changed(&e, "context", &context.id)
            .await
            .map_err(error)?;
        Ok(context)
    }
    async fn list_projects(&self) -> zbus::fdo::Result<Vec<ProjectInfo>> {
        self.store.projects().map_err(error)
    }
    async fn list_checkouts(&self) -> zbus::fdo::Result<Vec<CheckoutInfo>> {
        self.store.checkouts().map_err(error)
    }
    async fn list_contexts(&self) -> zbus::fdo::Result<Vec<ContextInfo>> {
        self.store.contexts().map_err(error)
    }
    async fn register_project(
        &self,
        cwd: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_context)] e: SignalContext<'_>,
    ) -> zbus::fdo::Result<ContextInfo> {
        let _lock = self.mutation.lock().await;
        let context = self.register(cwd, conn).await?;
        Self::changed(&e, "project", &context.project_id)
            .await
            .map_err(error)?;
        Ok(context)
    }
    async fn refresh(
        &self,
        cwd: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_context)] e: SignalContext<'_>,
    ) -> zbus::fdo::Result<ContextInfo> {
        let context = {
            let _lock = self.mutation.lock().await;
            self.register_inner(cwd, conn, false, true).await?
        };
        let checkout = self
            .store
            .checkouts()
            .map_err(error)?
            .into_iter()
            .find(|c| c.id == context.checkout_id)
            .ok_or_else(|| error("Missing checkout"))?;
        self.refresh_checkout(checkout.root_path, conn).await?;
        Self::changed(&e, "project", &context.project_id)
            .await
            .map_err(error)?;
        Ok(context)
    }
    async fn rename_project(
        &self,
        id: String,
        name: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_context)] e: SignalContext<'_>,
    ) -> zbus::fdo::Result<ProjectInfo> {
        if name.trim().is_empty() {
            return Err(zbus::fdo::Error::InvalidArgs("Name cannot be empty".into()));
        }
        let _lock = self.mutation.lock().await;
        let mut project = self
            .store
            .projects()
            .map_err(error)?
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| zbus::fdo::Error::FileNotFound("Project not found".into()))?;
        project.name = name;
        self.store.put("project", &id, &project).map_err(error)?;
        publish_project(conn, project.clone())
            .await
            .map_err(error)?;
        Self::changed(&e, "project", &id).await.map_err(error)?;
        Ok(project)
    }
    async fn list_goals(&self, date: String) -> zbus::fdo::Result<Vec<GoalInfo>> {
        Ok(self
            .store
            .goals()
            .map_err(error)?
            .into_iter()
            .filter(|g| date.is_empty() || g.date == date)
            .collect())
    }
    async fn create_goal(
        &self,
        info: GoalInfo,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_context)] e: SignalContext<'_>,
    ) -> zbus::fdo::Result<GoalInfo> {
        let goal: Goal = info
            .clone()
            .try_into()
            .map_err(zbus::fdo::Error::InvalidArgs)?;
        let _lock = self.mutation.lock().await;
        if let Some(project) = &goal.project {
            self.register(project.to_string_lossy().into_owned(), conn)
                .await?;
        }
        if !self
            .store
            .create("goal", &goal.key(), &info)
            .map_err(error)?
        {
            return Err(zbus::fdo::Error::FileExists("Goal already exists".into()));
        }
        publish_goal(conn, info.clone()).await.map_err(error)?;
        Self::changed(&e, "goal", &goal.key())
            .await
            .map_err(error)?;
        Ok(info)
    }
    async fn update_goal(
        &self,
        info: GoalInfo,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_context)] e: SignalContext<'_>,
    ) -> zbus::fdo::Result<GoalInfo> {
        let goal: Goal = info
            .clone()
            .try_into()
            .map_err(zbus::fdo::Error::InvalidArgs)?;
        let _lock = self.mutation.lock().await;
        if !self
            .store
            .goals()
            .map_err(error)?
            .iter()
            .any(|g| g.date == goal.date && g.id == goal.id)
        {
            return Err(zbus::fdo::Error::FileNotFound("Goal not found".into()));
        }
        if let Some(project) = &goal.project {
            self.register(project.to_string_lossy().into_owned(), conn)
                .await?;
        }
        self.store.put("goal", &goal.key(), &info).map_err(error)?;
        publish_goal(conn, info.clone()).await.map_err(error)?;
        Self::changed(&e, "goal", &goal.key())
            .await
            .map_err(error)?;
        Ok(info)
    }
    async fn remove_goal(
        &self,
        date: String,
        id: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_context)] e: SignalContext<'_>,
    ) -> zbus::fdo::Result<bool> {
        let _lock = self.mutation.lock().await;
        let key = format!("{date}/{id}");
        let removed = self.store.remove("goal", &key).map_err(error)?;
        if removed {
            conn.object_server()
                .remove::<GoalObject, _>(goal_path(&date, &id))
                .await
                .map_err(error)?;
            Self::changed(&e, "goal", &key).await.map_err(error)?;
        }
        Ok(removed)
    }
    #[zbus(signal)]
    async fn changed(e: &SignalContext<'_>, kind: &str, id: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn project_removed(e: &SignalContext<'_>, info: RemovedProjectInfo) -> zbus::Result<()>;
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let store = Arc::new(Store::open(&store::default_path())?);
    let (watch, requests) = tokio::sync::mpsc::unbounded_channel();
    let manager = Manager {
        store: store.clone(),
        mutation: Arc::new(Mutex::new(())),
        watch,
        checkout_mutations: Arc::new(Mutex::new(std::collections::HashMap::new())),
    };
    let conn = zbus::connection::Builder::session()?
        .serve_at(ROOT_PATH, zbus::fdo::ObjectManager)?
        .serve_at(ROOT_PATH, manager.clone())?
        .build()
        .await?;
    for p in store.projects()? {
        publish_project(&conn, p).await?;
    }
    for c in store.checkouts()? {
        publish_checkout(&conn, c).await?;
    }
    for c in store.contexts()? {
        conn.object_server()
            .at(path("Contexts", &c.id), ContextObject(c))
            .await?;
    }
    for g in store.goals()? {
        publish_goal(&conn, g).await?;
    }
    watch::start(manager.clone(), conn.clone(), requests)?;
    for checkout in store.checkouts()? {
        if let Some(dir) = git::git_dir(Path::new(&checkout.root_path)) {
            let (done, _) = tokio::sync::oneshot::channel();
            let _ = manager.watch.send((checkout.root_path, dir, false, done));
        }
    }
    conn.request_name(BUS_NAME).await?;
    eprintln!(
        "projd owns {BUS_NAME}; store {}",
        store::default_path().display()
    );
    tokio::signal::ctrl_c().await?;
    Ok(())
}
