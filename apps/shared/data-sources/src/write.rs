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
            eprintln!("[data-sources/dbus-write] {label}: {error}");
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

/// Runs a process and returns its stdout, reporting a non-zero status.
pub async fn output(
    program: impl AsRef<std::ffi::OsStr>,
    args: &[&str],
) -> Result<std::process::Output, String> {
    tokio::process::Command::new(program)
        .args(args)
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|error| format!("run process: {error}"))
}

/// Opens a URL through the external desktop opener without GTK activation.
pub async fn open_url(url: &str) -> Result<(), String> {
    let output = tokio::process::Command::new("xdg-open")
        .arg(url)
        .env_remove("XDG_ACTIVATION_TOKEN")
        .env_remove("DESKTOP_STARTUP_ID")
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|error| format!("could not launch URL opener: {error}"))?;
    output.status.success().then_some(()).ok_or_else(|| {
        format!(
            "URL opener exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )
    })
}

/// Copies text to the Wayland clipboard and primary selection.
pub fn copy_to_clipboard(text: String) {
    std::thread::spawn(move || {
        for primary in [false, true] {
            let mut command = std::process::Command::new("wl-copy");
            if primary {
                command.arg("--primary");
            }
            let result = command
                .stdin(std::process::Stdio::piped())
                .spawn()
                .and_then(|mut child| {
                    use std::io::Write;
                    child
                        .stdin
                        .as_mut()
                        .ok_or_else(|| {
                            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "no stdin")
                        })
                        .and_then(|stdin| stdin.write_all(text.as_bytes()))
                        .map(|_| child)
                });
            match result {
                Ok(mut child) => {
                    let _ = child.wait();
                }
                Err(error) => {
                    eprintln!("[branch-copy] wl-copy failed (primary={primary}): {error}");
                }
            }
        }
    });
}
