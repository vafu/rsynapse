//! Shared background D-Bus writes.
//!
//! Widgets call these instead of creating a runtime and connection per
//! mutation. One multi-thread runtime and one cached session/system connection
//! are reused process-wide.
use std::{future::Future, sync::OnceLock};

use zbus::Connection;

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
static SESSION: OnceLock<Connection> = OnceLock::new();
static SYSTEM: OnceLock<Connection> = OnceLock::new();

fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build shared D-Bus write runtime")
    })
}

/// Runs a D-Bus write off the GTK thread and logs failures.
pub fn spawn<F, E>(label: &'static str, future: F)
where
    F: Future<Output = Result<(), E>> + Send + 'static,
    E: std::fmt::Display + Send + 'static,
{
    runtime().spawn(async move {
        if let Err(error) = future.await {
            eprintln!("[shell-source/dbus-write] {label}: {error}");
        }
    });
}

pub async fn session_connection() -> zbus::Result<&'static Connection> {
    if let Some(connection) = SESSION.get() {
        return Ok(connection);
    }
    let connection = Connection::session().await?;
    let _ = SESSION.set(connection);
    Ok(SESSION.get().expect("session connection was just set"))
}

pub async fn system_connection() -> zbus::Result<&'static Connection> {
    if let Some(connection) = SYSTEM.get() {
        return Ok(connection);
    }
    let connection = Connection::system().await?;
    let _ = SYSTEM.set(connection);
    Ok(SYSTEM.get().expect("system connection was just set"))
}
