mod git;
mod icon;
mod migrate;
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
    fn cwd(&self) -> &str {
        &self.0.cwd
    }
    #[zbus(property)]
    fn checkout(&self) -> OwnedObjectPath {
        if self.0.checkout_id.is_empty() {
            OwnedObjectPath::try_from("/").unwrap()
        } else {
            path("Checkouts", &self.0.checkout_id)
        }
    }
    #[zbus(property)]
    fn icon(&self) -> &str {
        &self.0.icon
    }
    #[zbus(property)]
    fn icon_origin(&self) -> &str {
        &self.0.icon_origin
    }
}
struct CheckoutObject(CheckoutInfo, Arc<Store>);
#[interface(name = "org.rsynapse.Proj.Checkout1")]
impl CheckoutObject {
    #[zbus(property)]
    fn id(&self) -> &str {
        &self.0.id
    }
    #[zbus(property)]
    fn project(&self) -> zbus::fdo::Result<OwnedObjectPath> {
        Ok(self
            .1
            .projects()
            .map_err(error)?
            .into_iter()
            .find(|p| p.checkout_id == self.0.id)
            .map(|p| path("Projects", &p.id))
            .unwrap_or_else(|| OwnedObjectPath::try_from("/").unwrap()))
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
        if before.cwd != info.cwd {
            object.cwd_changed(handle.signal_context()).await?;
        }
        if before.checkout_id != info.checkout_id {
            object.checkout_changed(handle.signal_context()).await?;
        }
        if before.icon != info.icon {
            object.icon_changed(handle.signal_context()).await?;
        }
        if before.icon_origin != info.icon_origin {
            object.icon_origin_changed(handle.signal_context()).await?;
        }
    } else {
        conn.object_server().at(p, ProjectObject(info)).await?;
    }
    Ok(())
}
async fn publish_checkout(
    conn: &Connection,
    info: CheckoutInfo,
    store: Arc<Store>,
) -> zbus::Result<()> {
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
        conn.object_server()
            .at(p, CheckoutObject(info, store))
            .await?;
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
    async fn register(&self, cwd: String, conn: &Connection) -> zbus::fdo::Result<ProjectInfo> {
        self.register_inner(cwd, conn, true, false).await
    }
    async fn register_inner(
        &self,
        cwd: String,
        conn: &Connection,
        subscribe: bool,
        existing_only: bool,
    ) -> zbus::fdo::Result<ProjectInfo> {
        let (root, cwd) = tokio::task::spawn_blocking(move || git::discover(Path::new(&cwd)))
            .await
            .map_err(error)?
            .map_err(error)?;
        let cwd = cwd.to_string_lossy().into_owned();
        let root = root.map(|p| p.to_string_lossy().into_owned());
        let checkouts = self.store.checkouts().map_err(error)?;
        let existing_checkout = root
            .as_ref()
            .and_then(|root| checkouts.into_iter().find(|c| &c.root_path == root));
        let projects = self.store.projects().map_err(error)?;
        let existing = if let Some(checkout) = &existing_checkout {
            projects
                .iter()
                .find(|p| p.checkout_id == checkout.id)
                .cloned()
        } else if root.is_none() {
            projects
                .iter()
                .filter(|p| {
                    p.checkout_id.is_empty()
                        && (p.cwd == cwd || (existing_only && Path::new(&cwd).starts_with(&p.cwd)))
                })
                .max_by_key(|p| p.cwd.len())
                .cloned()
        } else {
            None
        };
        if existing_only && existing.is_none() {
            return Err(zbus::fdo::Error::FileNotFound(
                "Project is not registered; use proj add first".into(),
            ));
        }
        let mut project = existing.unwrap_or_else(|| ProjectInfo {
            id: id(),
            name: Path::new(root.as_deref().unwrap_or(&cwd))
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            cwd: cwd.clone(),
            checkout_id: String::new(),
            icon: String::new(),
            icon_origin: String::new(),
        });
        if !existing_only {
            project.cwd = cwd;
        }
        let mut checkout = None;
        if let Some(root) = root {
            let fresh = existing_checkout.is_none();
            let mut info = existing_checkout.unwrap_or_else(|| CheckoutInfo {
                id: id(),
                root_path: root.clone(),
                branch: String::new(),
                git_status: GitStatus::default(),
            });
            let owned = root.clone();
            info.branch = tokio::task::spawn_blocking(move || git::branch(Path::new(&owned)))
                .await
                .map_err(error)?;
            project.checkout_id = info.id.clone();
            checkout = Some((info, root, fresh));
        }
        self.store
            .put_project(&project, checkout.as_ref().map(|(info, _, _)| info))
            .map_err(error)?;
        publish_project(conn, project.clone())
            .await
            .map_err(error)?;
        if let Some((info, root, fresh)) = checkout {
            publish_checkout(conn, info, self.store.clone())
                .await
                .map_err(error)?;
            if let Some(dir) = git::git_dir(Path::new(&root)) {
                let (done, ready) = tokio::sync::oneshot::channel();
                let _ = self.watch.send((root, dir, fresh && subscribe, done));
                let _ = ready.await;
            }
        }
        Ok(project)
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
            publish_checkout(conn, checkout.clone(), self.store.clone())
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
    async fn set_project_icon(
        &self,
        id: String,
        glyph: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_context)] e: SignalContext<'_>,
    ) -> zbus::fdo::Result<ProjectInfo> {
        let p = self.update_icon(&id, glyph, false, false, conn).await?;
        Self::changed(&e, "project-icon", &id)
            .await
            .map_err(error)?;
        Ok(p)
    }
    async fn set_project_icon_if_unset(
        &self,
        id: String,
        glyph: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_context)] e: SignalContext<'_>,
    ) -> zbus::fdo::Result<ProjectInfo> {
        let p = self.update_icon(&id, glyph, true, false, conn).await?;
        Self::changed(&e, "project-icon", &id)
            .await
            .map_err(error)?;
        Ok(p)
    }
    async fn clear_project_icon(
        &self,
        id: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_context)] e: SignalContext<'_>,
    ) -> zbus::fdo::Result<ProjectInfo> {
        let p = self
            .update_icon(&id, String::new(), false, true, conn)
            .await?;
        Self::changed(&e, "project-icon", &id)
            .await
            .map_err(error)?;
        Ok(p)
    }
    async fn adopt_project_icon(
        &self,
        id: String,
        glyph: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_context)] e: SignalContext<'_>,
    ) -> zbus::fdo::Result<ProjectInfo> {
        if glyph.trim().is_empty() || glyph.len() > 64 {
            return Err(zbus::fdo::Error::InvalidArgs("Invalid icon glyph".into()));
        }
        let _lock = self.mutation.lock().await;
        let mut p = self
            .store
            .projects()
            .map_err(error)?
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| error("Project not found"))?;
        if p.icon_origin != "manual" {
            p.icon = glyph;
            p.icon_origin = "manual".into();
            self.store.put("project", &id, &p).map_err(error)?;
            publish_project(conn, p.clone()).await.map_err(error)?;
            Self::changed(&e, "project-icon", &id)
                .await
                .map_err(error)?;
        }
        Ok(p)
    }
    async fn resolve_project(
        &self,
        cwd: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_context)] e: SignalContext<'_>,
    ) -> zbus::fdo::Result<ProjectInfo> {
        let _lock = self.mutation.lock().await;
        let project = self.register_inner(cwd, conn, true, true).await?;
        Self::changed(&e, "project", &project.id)
            .await
            .map_err(error)?;
        Ok(project)
    }
    async fn list_projects(&self) -> zbus::fdo::Result<Vec<ProjectInfo>> {
        self.store.projects().map_err(error)
    }
    async fn list_checkouts(&self) -> zbus::fdo::Result<Vec<CheckoutInfo>> {
        self.store.checkouts().map_err(error)
    }
    async fn register_project(
        &self,
        cwd: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_context)] e: SignalContext<'_>,
    ) -> zbus::fdo::Result<ProjectInfo> {
        let _lock = self.mutation.lock().await;
        let project = self.register(cwd, conn).await?;
        Self::changed(&e, "project", &project.id)
            .await
            .map_err(error)?;
        Ok(project)
    }
    async fn refresh(
        &self,
        cwd: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_context)] e: SignalContext<'_>,
    ) -> zbus::fdo::Result<ProjectInfo> {
        let project = {
            let _lock = self.mutation.lock().await;
            self.register_inner(cwd, conn, false, true).await?
        };
        let checkout = self
            .store
            .checkouts()
            .map_err(error)?
            .into_iter()
            .find(|c| c.id == project.checkout_id);
        if let Some(checkout) = checkout {
            self.refresh_checkout(checkout.root_path, conn).await?;
        }
        Self::changed(&e, "project", &project.id)
            .await
            .map_err(error)?;
        Ok(project)
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
        publish_checkout(&conn, c, store.clone()).await?;
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
