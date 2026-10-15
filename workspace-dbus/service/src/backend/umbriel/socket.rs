use super::protocol::Event;
use anyhow::Context;
use serde::Deserialize;
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

pub(super) struct Socket(BufReader<UnixStream>);
impl Socket {
    pub async fn connect(path: Option<&Path>) -> anyhow::Result<Self> {
        let path = socket_path(
            path,
            std::env::var_os("UMBRIEL_SOCKET").as_deref(),
            std::env::var_os("XDG_RUNTIME_DIR").as_deref(),
            std::env::var_os("WAYLAND_DISPLAY").as_deref(),
        )?;
        let mut stream = UnixStream::connect(&path)
            .await
            .with_context(|| format!("connect to Umbriel at {}", path.display()))?;
        stream.write_all(b"{\"cmd\":\"subscribe\",\"events\":[\"outputs\",\"workspaces\",\"windows\",\"layers\",\"overview\"]}\n").await?;
        // Unlike Niri, subscription starts with events, with no Handled reply.
        // Keep the write half open as Umbriel's own subscriber does.
        Ok(Self(BufReader::new(stream)))
    }
    pub async fn event(&mut self) -> anyhow::Result<Event> {
        let mut line = String::new();
        anyhow::ensure!(
            self.0.read_line(&mut line).await? != 0,
            "Umbriel event stream closed"
        );
        decode(&line)
    }
}

fn decode(line: &str) -> anyhow::Result<Event> {
    match serde_json::from_str(line) {
        Ok(event) => Ok(event),
        Err(error) => {
            #[derive(Deserialize)]
            struct Rejected {
                err: String,
            }
            if let Ok(Rejected { err }) = serde_json::from_str(line) {
                anyhow::bail!("Umbriel rejected subscription: {err}");
            }
            // Serde's adjacent-tagged `other` variant only accepts unit
            // content. An unknown family may carry arbitrary JSON data.
            #[derive(Deserialize)]
            struct Header<'a> {
                event: &'a str,
            }
            if let Ok(Header { event }) = serde_json::from_str(line)
                && !["outputs", "workspaces", "windows", "layers", "overview"].contains(&event)
            {
                return Ok(Event::Unknown);
            }
            Err(error).context("invalid Umbriel event")
        }
    }
}

fn socket_path(
    explicit: Option<&Path>,
    socket: Option<&OsStr>,
    runtime: Option<&OsStr>,
    display: Option<&OsStr>,
) -> anyhow::Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.to_path_buf());
    }
    if let Some(path) = socket.filter(|v| !v.is_empty()) {
        return Ok(path.into());
    }
    let runtime = runtime
        .filter(|v| !v.is_empty())
        .context("UMBRIEL_SOCKET and XDG_RUNTIME_DIR are not set")?;
    let display = display
        .filter(|v| !v.is_empty())
        .context("UMBRIEL_SOCKET and WAYLAND_DISPLAY are not set")?;
    let mut name = OsString::from("umbriel-");
    name.push(display);
    name.push(".sock");
    Ok(Path::new(runtime).join(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_families_are_ignored_but_invalid_core_snapshots_are_rejected() {
        assert!(matches!(
            decode(r#"{"event":"future","data":{"new":[1,2]}}"#).unwrap(),
            Event::Unknown
        ));
        assert!(decode(r#"{"event":"windows","data":[{"id":"x"}]}"#).is_err());
        assert!(
            decode(r#"{"err":"unknown subscription"}"#)
                .unwrap_err()
                .to_string()
                .contains("unknown subscription")
        );
        assert!(decode("{broken").is_err());
    }
    #[test]
    fn explicit_and_exported_sockets_precede_the_display_fallback() {
        assert_eq!(
            socket_path(
                Some(Path::new("/explicit")),
                Some(OsStr::new("/exported")),
                None,
                None
            )
            .unwrap(),
            Path::new("/explicit")
        );
        assert_eq!(
            socket_path(None, Some(OsStr::new("/exported")), None, None).unwrap(),
            Path::new("/exported")
        );
        assert_eq!(
            socket_path(
                None,
                Some(OsStr::new("")),
                Some(OsStr::new("/run/user/1")),
                Some(OsStr::new("wayland-2"))
            )
            .unwrap(),
            Path::new("/run/user/1/umbriel-wayland-2.sock")
        );
        assert!(socket_path(None, None, None, None).is_err());
        assert!(socket_path(None, None, Some(OsStr::new("/run/user/1")), None).is_err());
    }
}
