//! Resolves the notebook scope for a command from projd and Remarked.
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use notebook_api::notebook_scope::NotebookScope;
use proj_model::api::{ManagerProxy, MetricsProxy};
use proj_model::{GoalIdentityInfo, ProjectInfo};

use crate::output::ScopeView;
use crate::tasknotes;

pub struct Resolved {
    pub project: Option<ProjectInfo>,
    pub task: Option<GoalIdentityInfo>,
    pub scope: NotebookScope,
}

impl Resolved {
    pub fn view(&self) -> ScopeView {
        ScopeView {
            notebook: self.scope.path.clone(),
            tags: self.scope.normalized_tags(),
        }
    }

    pub fn task_required(&self) -> Result<&GoalIdentityInfo> {
        self.task.as_ref().ok_or_else(|| {
            crate::output::CliError::invalid(
                "no task resolved; pass --task, set `rnotes task checkpoint KEY`, or `rnotes task start KEY`",
            )
            .into()
        })
    }
}

pub async fn resolve(
    connection: &zbus::Connection,
    project: Option<&Path>,
    task: Option<&str>,
    notebook: Option<&Path>,
    global: bool,
    extra_tags: &[String],
) -> Result<Resolved> {
    let explicit_notebook = notebook.map(absolute).transpose()?;
    let project = match (&explicit_notebook, global) {
        (Some(_), _) | (None, true) => None,
        (None, false) => Some(resolve_project(connection, project).await?),
    };
    let path = match (explicit_notebook, project.as_ref()) {
        (Some(path), _) => path,
        (None, Some(project)) => task_notes::notebook_path(&project.cwd),
        (None, None) => PathBuf::new(),
    };
    let task = match task {
        Some(reference) => {
            let metrics = MetricsProxy::new(connection)
                .await
                .context("connect to projd metrics")?;
            Some(resolve_task(&metrics, project.as_ref(), reference).await?)
        }
        None => None,
    };
    let task = match (task, project.as_ref(), global) {
        (Some(task), _, _) => Some(task),
        (None, Some(project), false) => implicit_task(connection, project).await?,
        (None, _, _) => None,
    };
    let mut tags = extra_tags.to_vec();
    if let Some(task) = &task {
        tags.push(task_notes::task_tag(&task.instance_id));
    }
    let scope = NotebookScope {
        path: path.to_string_lossy().into_owned(),
        tags,
    };
    Ok(Resolved {
        project,
        task,
        scope,
    })
}

const CHECKPOINT_FILE: &str = ".rnotes-task";

/// Implicit task for project-scoped commands: the checkpoint file wins, then the
/// project's active task. Both are best-effort; absence is not an error.
async fn implicit_task(
    connection: &zbus::Connection,
    project: &ProjectInfo,
) -> Result<Option<GoalIdentityInfo>> {
    if let Some(reference) = read_checkpoint(&project.cwd)? {
        let metrics = MetricsProxy::new(connection)
            .await
            .context("connect to projd metrics")?;
        return Ok(Some(
            resolve_task(&metrics, Some(project), &reference).await?,
        ));
    }
    let metrics = MetricsProxy::new(connection)
        .await
        .context("connect to projd metrics")?;
    let tracking = metrics
        .list_goal_tracking()
        .await
        .context("list active projd tasks")?;
    let selected = tracking
        .into_iter()
        .find(|row| row.selected && row.project_id == project.id)
        .map(|row| row.instance_id);
    let Some(instance_id) = selected else {
        return Ok(None);
    };
    Ok(resolve_task(&metrics, Some(project), &instance_id)
        .await
        .ok())
}

fn checkpoint_path(project_cwd: &str) -> PathBuf {
    Path::new(project_cwd).join(CHECKPOINT_FILE)
}

pub fn read_checkpoint(project_cwd: &str) -> Result<Option<String>> {
    let path = checkpoint_path(project_cwd);
    match std::fs::read_to_string(&path) {
        Ok(contents) => {
            let value = contents.trim().to_string();
            Ok((!value.is_empty()).then_some(value))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("read {}", path.display())),
    }
}

pub fn write_checkpoint(project_cwd: &str, reference: &str) -> Result<PathBuf> {
    let path = checkpoint_path(project_cwd);
    tasknotes::atomic_write(&path, &format!("{reference}\n"))?;
    Ok(path)
}

pub fn clear_checkpoint(project_cwd: &str) -> Result<PathBuf> {
    let path = checkpoint_path(project_cwd);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(path),
        Err(error) => Err(error).with_context(|| format!("remove {}", path.display())),
    }
}

async fn resolve_project(
    connection: &zbus::Connection,
    path: Option<&Path>,
) -> Result<ProjectInfo> {
    let manager = ManagerProxy::new(connection)
        .await
        .context("connect to projd")?;
    let path = absolute(path.unwrap_or_else(|| Path::new(".")))?;
    manager
        .resolve_project(&path.to_string_lossy())
        .await
        .with_context(|| {
            format!(
                "resolve registered project at {}; run `proj add {}` first",
                path.display(),
                path.display()
            )
        })
}

async fn resolve_task(
    metrics: &MetricsProxy<'_>,
    project: Option<&ProjectInfo>,
    reference: &str,
) -> Result<GoalIdentityInfo> {
    let rows = metrics
        .list_goal_identities(reference)
        .await
        .with_context(|| format!("resolve projd task {reference}"))?;
    select_task(rows, project, reference)
}

fn select_task(
    rows: Vec<GoalIdentityInfo>,
    project: Option<&ProjectInfo>,
    reference: &str,
) -> Result<GoalIdentityInfo> {
    let mut matches: Vec<_> = rows
        .into_iter()
        .filter(|row| {
            row.instance_id == reference || row.key == reference || row.goal.id == reference
        })
        .filter(|row| row.deleted_at_ms == 0)
        .collect();
    if matches.is_empty() {
        bail!("task not found: {reference}");
    }
    if let Some(project) = project {
        matches.retain(|row| row.goal.project_id == project.id);
        if matches.is_empty() {
            bail!(
                "task {reference} does not belong to project {}",
                project.name
            );
        }
    }
    match matches.as_slice() {
        [task] => Ok(task.clone()),
        _ => bail!(
            "task reference {reference} is ambiguous; use its immutable instance UUID from `proj goal instances {reference} --json`"
        ),
    }
}

fn absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(std::env::current_dir()?.join(path))
}

#[cfg(test)]
mod test;
