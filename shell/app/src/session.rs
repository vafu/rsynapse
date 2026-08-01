use std::{fs, os::unix::fs::MetadataExt};

use futures_util::StreamExt;
use shell_core::source::{
    self, Observable,
    dbus::{self, Bus, ObjectDescriptor, PropertyDescriptor},
    rx::Observable as _,
};
use zbus::{Proxy, zvariant::OwnedObjectPath};

const LOGIND_BUS: &str = "org.freedesktop.login1";
const LOGIND_MANAGER_PATH: &str = "/org/freedesktop/login1";
const LOGIND_MANAGER_INTERFACE: &str = "org.freedesktop.login1.Manager";
const LOGIND_SESSION_INTERFACE: &str = "org.freedesktop.login1.Session";
const WAYLAND_SESSION_TYPE: &str = "wayland";

type LoginSessionRow = (String, u32, String, String, OwnedObjectPath);

#[derive(Clone, Debug, Eq, PartialEq)]
struct SessionSnapshot {
    uid: u32,
    path: String,
    active: bool,
    session_type: String,
}

pub fn locked() -> Observable<bool> {
    source::shared_by_key("rsynapse.session-locked", "active-wayland", || {
        source::from_task(|sender| async move {
            let descriptor = match locked_hint_descriptor().await {
                Ok(descriptor) => descriptor,
                Err(error) => {
                    let _ = sender.send(Err(error)).await;
                    return;
                }
            };

            let mut stream = dbus::property_or::<bool>(descriptor, false).into_stream();
            while let Some(item) = stream.next().await {
                if sender.send(item).await.is_err() {
                    return;
                }
            }
        })
        .distinct_until_changed()
        .box_it()
    })
}

async fn locked_hint_descriptor() -> Result<PropertyDescriptor, String> {
    let path = current_session_path().await?;
    let object = ObjectDescriptor::parse(
        Bus::System,
        LOGIND_BUS,
        path.as_str(),
        LOGIND_SESSION_INTERFACE,
    )?;
    Ok(PropertyDescriptor::new(object, "LockedHint"))
}

async fn current_session_path() -> Result<String, String> {
    let uid = current_uid()?;
    let connection = zbus::Connection::system()
        .await
        .map_err(|error| format!("connect logind system bus failed: {error}"))?;
    let manager = Proxy::new(
        &connection,
        LOGIND_BUS,
        LOGIND_MANAGER_PATH,
        LOGIND_MANAGER_INTERFACE,
    )
    .await
    .map_err(|error| format!("connect logind manager failed: {error}"))?;
    let rows: Vec<LoginSessionRow> = manager
        .call("ListSessions", &())
        .await
        .map_err(|error| format!("list logind sessions failed: {error}"))?;

    let mut sessions = Vec::new();
    for row in rows.into_iter().filter(|row| row.1 == uid) {
        match session_snapshot(&connection, row).await {
            Ok(session) => sessions.push(session),
            Err(error) => eprintln!("[session] skipped logind session: {error}"),
        }
    }

    select_session_path(&sessions, uid)
        .map(str::to_owned)
        .ok_or_else(|| format!("no active wayland logind session found for uid {uid}"))
}

async fn session_snapshot(
    connection: &zbus::Connection,
    (_id, uid, _user, _seat, path): LoginSessionRow,
) -> Result<SessionSnapshot, String> {
    let proxy = Proxy::new(
        connection,
        LOGIND_BUS,
        path.as_str(),
        LOGIND_SESSION_INTERFACE,
    )
    .await
    .map_err(|error| format!("connect logind session {} failed: {error}", path.as_str()))?;
    let active = proxy
        .get_property::<bool>("Active")
        .await
        .map_err(|error| format!("read logind Active for {} failed: {error}", path.as_str()))?;
    let session_type = proxy
        .get_property::<String>("Type")
        .await
        .map_err(|error| format!("read logind Type for {} failed: {error}", path.as_str()))?;

    Ok(SessionSnapshot {
        uid,
        path: path.to_string(),
        active,
        session_type,
    })
}

fn select_session_path(sessions: &[SessionSnapshot], uid: u32) -> Option<&str> {
    sessions
        .iter()
        .find(|session| {
            session.uid == uid && session.active && session.session_type == WAYLAND_SESSION_TYPE
        })
        .map(|session| session.path.as_str())
}

fn current_uid() -> Result<u32, String> {
    fs::metadata("/proc/self")
        .map(|metadata| metadata.uid())
        .map_err(|error| format!("read current uid failed: {error}"))
}

#[cfg(test)]
mod test;
