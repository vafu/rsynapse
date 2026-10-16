use std::future::Future;

use anyhow::{Result, bail};
use futures_util::{StreamExt, stream::SelectAll};
use workspace_dbus_api::{ManagerProxy, WindowProxy};
use zbus::{Connection, proxy::PropertyStream, zvariant::OwnedObjectPath};

struct Window {
    proxy: WindowProxy<'static>,
    title: PropertyStream<'static, Vec<String>>,
}

async fn windows(connection: &Connection, paths: Vec<OwnedObjectPath>) -> Result<Vec<Window>> {
    let mut windows = Vec::new();
    for path in paths {
        let proxy = WindowProxy::builder(connection).path(path)?.build().await?;
        let title = proxy.receive_title_changed().await;
        windows.push(Window { proxy, title });
    }
    Ok(windows)
}

async fn matching_id(windows: &[Window], token: &str) -> Result<Option<String>> {
    let mut found = None;
    for window in windows {
        // A window may disappear while its removal signal is in flight.
        let Ok(title) = window.proxy.title().await else {
            continue;
        };
        if title.first().map(String::as_str) != Some(token) {
            continue;
        }
        let app_id = window.proxy.app_id().await?;
        if !app_id
            .iter()
            .any(|id| id == "com.mitchellh.ghostty" || id == "ghostty")
        {
            continue;
        }
        let id = window.proxy.id().await?;
        if id.is_empty() {
            bail!("terminal has an empty window ID");
        }
        if found.replace(id).is_some() {
            bail!("terminal title challenge matched multiple windows");
        }
    }
    Ok(found)
}

/// Subscribe before publishing a fresh OSC title challenge to the owning TTY.
/// Neither focus nor the terminal process PID identifies a particular window.
pub async fn resolve<F, Fut>(connection: &Connection, token: &str, publish: F) -> Result<String>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let manager = ManagerProxy::new(connection).await?;
    let mut inventory = manager.receive_windows_changed().await;
    let mut owner = manager.inner().receive_owner_changed().await?;
    let generation = manager.generation().await?;
    if !manager.connected().await? {
        bail!("workspace inventory is not connected");
    }
    let mut current = windows(connection, manager.windows().await?).await?;
    publish().await?;
    loop {
        if let Some(id) = matching_id(&current, token).await? {
            if !manager.connected().await? || manager.generation().await? != generation {
                bail!("workspace inventory reset during terminal discovery");
            }
            return Ok(id);
        }
        let mut titles: SelectAll<_> = current.iter_mut().map(|window| &mut window.title).collect();
        tokio::select! {
            changed = titles.next(), if !titles.is_empty() => {
                if changed.is_none() {
                    bail!("window title subscription ended");
                }
            }
            changed = inventory.next() => {
                let paths = changed.ok_or_else(|| anyhow::anyhow!("window inventory subscription ended"))?.get().await?;
                drop(titles);
                current = windows(connection, paths).await?;
            }
            _ = owner.next() => bail!("workspace service owner changed during terminal discovery"),
        }
    }
}
