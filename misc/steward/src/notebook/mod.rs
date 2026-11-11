//! Focused-project notebook selection for the remarked UI.

mod policy;

use std::path::{Path, PathBuf};

use data_sources::rx::Observable as _;
use futures_util::StreamExt;

pub(crate) use policy::notebook_scope;

/// Push the focused project's notebook, or the default notebook, into the
/// remarked UI whenever the selection changes.
pub(crate) async fn run() -> anyhow::Result<()> {
    let mut projects = data_sources::workspace_project::focused_project(None).into_stream();
    let mut last: Option<Option<PathBuf>> = None;
    while let Some(project) = projects.next().await {
        let Ok(project) = project else { continue };
        let scope = notebook_scope(project.as_ref());
        if last.as_ref() == Some(&scope) {
            continue;
        }
        if push(scope.as_deref()).await.is_ok() {
            last = Some(scope);
        }
    }
    Ok(())
}

async fn push(scope: Option<&Path>) -> anyhow::Result<()> {
    let connection = zbus::Connection::session().await?;
    let proxy = steward_api::RemarkedUiProxy::new(&connection).await?;
    proxy
        .open_notebook((
            scope.map(|path| path.display().to_string()).unwrap_or_default(),
            Vec::new(),
        ))
        .await?;
    Ok(())
}
