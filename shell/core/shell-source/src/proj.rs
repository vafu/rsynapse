//! UI-free reactive clients for the reusable project-management service.
use crate::{Observable, rx::Observable as _};
use futures_util::StreamExt;
use proj_model::{BUS_NAME, MANAGER_INTERFACE, ROOT_PATH, object_path};
pub use proj_model::{CheckoutInfo, GoalInfo, ProjectInfo, RemovedProjectInfo};
use zbus::Proxy;

fn snapshots<T>(method: &'static str, goal_list: bool) -> Observable<Vec<T>>
where
    T: serde::de::DeserializeOwned + zbus::zvariant::Type + Clone + Send + 'static,
{
    crate::shared_by_key("projd-snapshots", method.to_owned(), move || {
        crate::from_task(move |sender| async move {
            let run = async {
                let conn = zbus::Connection::session()
                    .await
                    .map_err(|e| e.to_string())?;
                let proxy = Proxy::new(&conn, BUS_NAME, ROOT_PATH, MANAGER_INTERFACE)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut changes = proxy
                    .receive_signal("Changed")
                    .await
                    .map_err(|e| e.to_string())?;
                loop {
                    let rows: Vec<T> = if goal_list {
                        proxy.call(method, &(String::new(),)).await
                    } else {
                        proxy.call(method, &()).await
                    }
                    .map_err(|e| e.to_string())?;
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
}
pub fn projects() -> Observable<Vec<ProjectInfo>> {
    snapshots("ListProjects", false)
        .distinct_until_changed()
        .box_it()
}
pub fn removed_projects() -> Observable<RemovedProjectInfo> {
    crate::from_task(|sender| async move {
        let run = async {
            let conn = zbus::Connection::session()
                .await
                .map_err(|e| e.to_string())?;
            let proxy = Proxy::new(&conn, BUS_NAME, ROOT_PATH, MANAGER_INTERFACE)
                .await
                .map_err(|e| e.to_string())?;
            let mut events = proxy
                .receive_signal("ProjectRemoved")
                .await
                .map_err(|e| e.to_string())?;
            while let Some(event) = events.next().await {
                let (info,) = event
                    .body()
                    .deserialize::<(RemovedProjectInfo,)>()
                    .map_err(|e| e.to_string())?;
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
    snapshots("ListCheckouts", false)
        .distinct_until_changed()
        .box_it()
}
pub fn goals() -> Observable<Vec<GoalInfo>> {
    snapshots("ListGoals", true)
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
