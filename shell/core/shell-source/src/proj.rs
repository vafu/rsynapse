//! UI-free reactive clients for the reusable project-management service.
use crate::{Observable, rx::Observable as _};
use futures_util::StreamExt;
pub use proj_model::api::{ManagerProxy, MetricsProxy};
use proj_model::{BUS_NAME, object_path};
pub use proj_model::{CheckoutInfo, GoalIdentityInfo, GoalInfo, ProjectInfo, RemovedProjectInfo};
mod tracking;
pub use tracking::goal_tracking;
mod labels;
pub use labels::project_labels;

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

macro_rules! metrics_snapshot {
    ($key:expr, $list:ident, $arg:expr) => {{
        crate::shared_by_key("projd-snapshots", $key, move || {
            crate::from_task(move |sender| async move {
                let run = async {
                    let conn = zbus::Connection::session()
                        .await
                        .map_err(|e| e.to_string())?;
                    let proxy = MetricsProxy::new(&conn).await.map_err(|e| e.to_string())?;
                    let mut changes = proxy
                        .receive_work_changed()
                        .await
                        .map_err(|e| e.to_string())?;
                    loop {
                        let rows = proxy.$list($arg).await.map_err(|e| e.to_string())?;
                        if sender.send(Ok(rows)).await.is_err() {
                            return Ok::<(), String>(());
                        }
                        if changes.next().await.is_none() {
                            return Err("projd work stream ended".to_owned());
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
pub fn goals() -> Observable<Vec<GoalInfo>> {
    manager_snapshot!("manager:ListGoals", list_goals, "")
        .distinct_until_changed()
        .box_it()
}
/// Task-instance snapshots, including creation timestamps and retained deleted instances.
/// Refreshed by Metrics1 work changes; consumers choose whether to include deleted tasks.
pub fn goal_identities() -> Observable<Vec<GoalIdentityInfo>> {
    metrics_snapshot!("metrics:ListGoalIdentities", list_goal_identities, "")
        .distinct_until_changed()
        .box_it()
}
pub fn goal_title(key: impl Into<String>) -> Observable<String> {
    goal_property(key.into(), "Title")
}
pub fn goal_status(key: impl Into<String>) -> Observable<String> {
    goal_property(key.into(), "Status")
}
pub fn goal_description(key: impl Into<String>) -> Observable<String> {
    goal_property(key.into(), "Description")
}
fn goal_property(key: String, property: &'static str) -> Observable<String> {
    let (date, id) = key.split_once('/').unwrap_or(("", &key));
    let descriptor = crate::dbus::ObjectDescriptor::parse(
        crate::dbus::Bus::Session,
        BUS_NAME,
        &proj_model::goal_path(date, id),
        proj_model::GOAL_INTERFACE,
    )
    .expect("goal descriptor");
    crate::dbus::property::<String>(crate::dbus::PropertyDescriptor::new(descriptor, property))
        .filter_map(|value| value)
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
pub fn active_goal(id: impl Into<String>) -> Observable<zbus::zvariant::OwnedObjectPath> {
    let descriptor = crate::dbus::ObjectDescriptor::parse(
        crate::dbus::Bus::Session,
        BUS_NAME,
        &object_path("Projects", &id.into()),
        proj_model::PROJECT_INTERFACE,
    )
    .expect("project descriptor");
    crate::dbus::property_or(
        crate::dbus::PropertyDescriptor::new(descriptor, "ActiveGoal"),
        zbus::zvariant::OwnedObjectPath::try_from("/").unwrap(),
    )
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
