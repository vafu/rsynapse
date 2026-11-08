use anyhow::Result;
use serde_json::json;

use crate::Session;
use crate::output::Output;
use crate::scope::Resolved;

pub async fn status(session: &Session, resolved: &Resolved) -> Result<Output> {
    let mut status = Session::status(&session.connection).await?;
    if let Some(path) = resolved.scope.storage_path() {
        status.notebook_dir = path.to_string();
    }
    let tags = resolved.scope.normalized_tags();
    let text = format!(
        "notebook: {}\nconfigured: {}\nzk: {}\nnotes: {}\nproject: {}\ntask: {}\nscope tags: {}",
        status.notebook_dir,
        status.configured,
        if status.zk_ok { "ok" } else { "error" },
        status.note_count,
        resolved
            .project
            .as_ref()
            .map(|project| project.name.as_str())
            .unwrap_or("-"),
        resolved
            .task
            .as_ref()
            .map(|task| task.key.as_str())
            .unwrap_or("-"),
        if tags.is_empty() {
            "-".to_string()
        } else {
            tags.join(", ")
        }
    );
    Ok(Output {
        command: "status",
        data: json!({
            "status": status,
            "project": resolved.project,
            "task": resolved.task,
            "scope": resolved.view(),
        }),
        page: None,
        text,
    })
}

pub async fn capabilities(session: &Session) -> Result<Output> {
    let capabilities = session.capabilities().await?;
    Ok(Output {
        command: "capabilities",
        data: serde_json::to_value(&capabilities)?,
        page: None,
        text: capabilities.names.join("\n"),
    })
}
