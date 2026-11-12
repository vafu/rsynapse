//! UI-free reactive clients for the reusable project-management service.
use crate::{Observable, rx::Observable as _};
use futures_util::StreamExt;
pub use proj_model::api::ManagerProxy;
use proj_model::{BUS_NAME, object_path};
pub use proj_model::{CheckoutInfo, ProjectInfo, RemovedProjectInfo};
mod labels;
pub use labels::project_labels;

/// Active task projected from remarked's `active/<project>` tag.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActiveTask {
    pub id: String,
    pub title: String,
    pub status: String,
    pub project: String,
}

#[zbus::proxy(
    interface = "org.remarked.Tasks1",
    default_service = "org.remarked.Service",
    default_path = "/org/remarked/Tasks"
)]
trait RemarkedTasks {
    fn get_active_task(&self, project: &str) -> zbus::Result<Vec<RemarkedTask>>;

    #[zbus(signal)]
    fn tasks_changed(&self, notebook_dir: String) -> zbus::Result<()>;
}

#[allow(dead_code)]
#[derive(Clone, Debug, Default, PartialEq, Eq, zbus::zvariant::Type, serde::Deserialize)]
struct RemarkedTask {
    id: String,
    revision: String,
    title: String,
    path: String,
    status: u32,
    priority: u32,
    active: bool,
    kind: String,
    project: String,
    parent_ids: Vec<String>,
    child_ids: Vec<String>,
    created: String,
    modified: String,
    preview: String,
}

impl From<RemarkedTask> for ActiveTask {
    fn from(task: RemarkedTask) -> Self {
        Self {
            id: task.id,
            title: task.title,
            status: match task.status {
                1 => "in-progress",
                2 => "done",
                3 => "cancelled",
                _ => "todo",
            }
            .to_owned(),
            project: task.project,
        }
    }
}

async fn remarked_tasks(notebook: Option<String>) -> Result<RemarkedTasksProxy<'static>, String> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|error| format!("connect remarked: {error}"))?;
    let notebooks = zbus::Proxy::new(
        &connection,
        "org.remarked.Service",
        "/org/remarked/Notebooks",
        "org.remarked.Notebooks1",
    )
    .await
    .map_err(|error| format!("connect remarked notebooks: {error}"))?;
    let handle: (String,) = notebooks
        .call(
            "OpenNotebook",
            &((notebook.unwrap_or_default(), Vec::<String>::new()),),
        )
        .await
        .map_err(|error| format!("open remarked notebook: {error}"))?;
    let path = format!("{}/Tasks", handle.0.trim_end_matches('/'));
    RemarkedTasksProxy::builder(&connection)
        .destination("org.remarked.Service")
        .map_err(|error| error.to_string())?
        .path(path)
        .map_err(|error| error.to_string())?
        .build()
        .await
        .map_err(|error| format!("connect remarked tasks: {error}"))
}

/// Observe the active task for a project. The project argument is the
/// remarked project facet; for projd-backed consumers this is the project id.
pub fn active_task(project: impl Into<String>) -> Observable<Option<ActiveTask>> {
    let project = project.into();
    crate::shared_by_key("remarked-active-task", project.clone(), move || {
        let project = project.clone();
        crate::from_task(move |sender| {
            let project = project.clone();
            async move {
                let run = async {
                    let tasks = remarked_tasks(None).await?;
                    let mut changes = tasks
                        .receive_tasks_changed()
                        .await
                        .map_err(|error| format!("subscribe remarked tasks: {error}"))?;
                    let mut previous = None;
                    loop {
                        let task = tasks
                            .get_active_task(&project)
                            .await
                            .map_err(|error| format!("get active task: {error}"))?
                            .into_iter()
                            .next()
                            .map(ActiveTask::from);
                        if task != previous {
                            previous = task.clone();
                            if sender.send(Ok(task)).await.is_err() {
                                return Ok::<(), String>(());
                            }
                        }
                        if changes.next().await.is_none() {
                            return Err("remarked task signal stream ended".to_owned());
                        }
                    }
                }
                .await;
                if let Err(error) = run {
                    let _ = sender.send(Err(error)).await;
                }
            }
        })
        .box_it()
    })
}

macro_rules! manager_snapshot {
    ($key:expr, $list:ident) => {{
        crate::shared_by_key("projd-snapshots", $key, move || {
            crate::from_task(move |sender| async move {
                let run = async {
                    let conn = zbus::Connection::session()
                        .await
                        .map_err(|e| e.to_string())?;
                    let proxy = ManagerProxy::new(&conn).await.map_err(|e| e.to_string())?;
                    let mut changes = proxy.receive_changed().await.map_err(|e| e.to_string())?;
                    loop {
                        let rows = proxy.$list().await.map_err(|e| e.to_string())?;
                        if sender.send(Ok(rows)).await.is_err() {
                            return Ok::<(), String>(());
                        }
                        if changes.next().await.is_none() {
                            return Err("projd change stream ended".to_owned());
                        }
                    }
                }
                .await;
                if let Err(error) = run {
                    let _ = sender.send(Err(error)).await;
                }
            })
            .box_it()
        })
    }};
    ($key:expr, $list:ident, $arg:expr) => {{
        crate::shared_by_key("projd-snapshots", $key, move || {
            crate::from_task(move |sender| async move {
                let run = async {
                    let conn = zbus::Connection::session()
                        .await
                        .map_err(|e| e.to_string())?;
                    let proxy = ManagerProxy::new(&conn).await.map_err(|e| e.to_string())?;
                    let mut changes = proxy.receive_changed().await.map_err(|e| e.to_string())?;
                    loop {
                        let rows = proxy.$list($arg).await.map_err(|e| e.to_string())?;
                        if sender.send(Ok(rows)).await.is_err() {
                            return Ok::<(), String>(());
                        }
                        if changes.next().await.is_none() {
                            return Err("projd change stream ended".to_owned());
                        }
                    }
                }
                .await;
                if let Err(error) = run {
                    let _ = sender.send(Err(error)).await;
                }
            })
            .box_it()
        })
    }};
}

pub fn projects() -> Observable<Vec<ProjectInfo>> {
    manager_snapshot!("manager:ListProjects", list_projects)
        .distinct_until_changed()
        .box_it()
}
pub fn removed_projects() -> Observable<RemovedProjectInfo> {
    crate::from_task(|sender| async move {
        let run = async {
            let conn = zbus::Connection::session()
                .await
                .map_err(|e| e.to_string())?;
            let proxy = ManagerProxy::new(&conn).await.map_err(|e| e.to_string())?;
            let mut events = proxy
                .receive_project_removed()
                .await
                .map_err(|e| e.to_string())?;
            while let Some(event) = events.next().await {
                let info = event.args().map_err(|e| e.to_string())?.info;
                if sender.send(Ok(info)).await.is_err() {
                    return Ok::<_, String>(());
                }
            }
            Ok(())
        }
        .await;
        if let Err(error) = run {
            let _ = sender.send(Err(error)).await;
        }
    })
}
pub fn checkouts() -> Observable<Vec<CheckoutInfo>> {
    manager_snapshot!("manager:ListCheckouts", list_checkouts)
        .distinct_until_changed()
        .box_it()
}
pub fn project_name(id: impl Into<String>) -> Observable<String> {
    let descriptor = crate::dbus::ObjectDescriptor::parse(
        crate::dbus::Bus::Session,
        BUS_NAME,
        &object_path("Projects", &id.into()),
        proj_model::PROJECT_INTERFACE,
    )
    .expect("project descriptor");
    crate::dbus::property_or(
        crate::dbus::PropertyDescriptor::new(descriptor, "Name"),
        String::new(),
    )
}
pub fn project_icon(id: impl Into<String>) -> Observable<String> {
    project_property(id.into(), "Icon")
}
pub fn project_icon_origin(id: impl Into<String>) -> Observable<String> {
    project_property(id.into(), "IconOrigin")
}
fn project_property(id: String, name: &'static str) -> Observable<String> {
    let descriptor = crate::dbus::ObjectDescriptor::parse(
        crate::dbus::Bus::Session,
        BUS_NAME,
        &object_path("Projects", &id),
        proj_model::PROJECT_INTERFACE,
    )
    .expect("project descriptor");
    crate::dbus::property::<String>(crate::dbus::PropertyDescriptor::new(descriptor, name))
        .filter_map(|value| value)
        .box_it()
}
pub fn checkout_branch(id: impl Into<String>) -> Observable<String> {
    let descriptor = crate::dbus::ObjectDescriptor::parse(
        crate::dbus::Bus::Session,
        BUS_NAME,
        &object_path("Checkouts", &id.into()),
        proj_model::CHECKOUT_INTERFACE,
    )
    .expect("checkout descriptor");
    crate::dbus::property_or(
        crate::dbus::PropertyDescriptor::new(descriptor, "Branch"),
        String::new(),
    )
}

/// Initializes or binds the focused workspace to a project path.
pub async fn bind_workspace_project(
    workspace: String,
    path: &str,
    initialize: bool,
) -> Result<ProjectInfo, String> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    let proxy = crate::steward::AssociationsProxy::new(&connection)
        .await
        .map_err(|error| format!("connect steward associations: {error}"))?;
    if initialize {
        proxy
            .init_workspace_project(&workspace, path)
            .await
            .map_err(|error| format!("initialize workspace project: {error}"))
    } else {
        proxy
            .bind_workspace_project(&workspace, path)
            .await
            .map_err(|error| format!("bind workspace project: {error}"))
    }
}

pub async fn init_workspace_id() -> Result<String, String> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    crate::steward::AssociationsProxy::new(&connection)
        .await
        .map_err(|error| format!("connect steward associations: {error}"))?
        .get_init_workspace()
        .await
        .map_err(|error| format!("get init workspace: {error}"))
}

pub async fn unassign_workspace_project(workspace: String) -> Result<(), String> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    crate::steward::AssociationsProxy::new(&connection)
        .await
        .map_err(|error| format!("connect steward associations: {error}"))?
        .unassign_workspace_project(&workspace)
        .await
        .map_err(|error| format!("unassign workspace project: {error}"))
}

pub async fn remove_project(id: String) -> Result<(), String> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    let proxy = ManagerProxy::new(&connection)
        .await
        .map_err(|error| format!("connect projd manager: {error}"))?;
    let _: bool = proxy
        .remove_project(&id)
        .await
        .map_err(|error| format!("remove project {id}: {error}"))?;
    Ok(())
}

pub fn set_project_icon(id: String, glyph: Option<String>) {
    crate::write::spawn("project icon", async move {
        let conn = crate::write::session_connection().await?;
        let proxy = ManagerProxy::new(conn).await?;
        let _: ProjectInfo = match glyph {
            Some(glyph) => proxy.set_project_icon(&id, &glyph).await?,
            None => proxy.clear_project_icon(&id).await?,
        };
        Ok::<_, zbus::Error>(())
    });
}

pub async fn set_project_icon_if_unset(id: String, glyph: String) -> Result<(), String> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    let proxy = ManagerProxy::new(&connection)
        .await
        .map_err(|error| format!("connect projd manager: {error}"))?;
    let _: ProjectInfo = proxy
        .set_project_icon_if_unset(&id, &glyph)
        .await
        .map_err(|error| format!("set project icon if unset: {error}"))?;
    Ok(())
}
