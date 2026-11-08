//! projd project-task commands.
//!
//! These are the same domain operations as `proj goal`, exposed here so a single
//! client can operate tasks and their Remarked notes together.
use crate::Session;
use crate::args::{
    ProjectTaskCommand, TaskActiveArgs, TaskCheckpointArgs, TaskCreateArgs, TaskListArgs,
    TaskNoteCommand, TaskRefArgs, TaskStatusArgs, TaskUpdateArgs,
};
use crate::output::{CliError, Output};
use crate::scope::Resolved;
use anyhow::{Context, Result, bail};
use proj_model::api::{ManagerProxy, MetricsProxy};
use proj_model::{Goal, GoalInfo, ProjectInfo};
use std::collections::HashMap;

pub async fn execute(
    command: ProjectTaskCommand,
    session: &Session,
    resolved: &Resolved,
) -> Result<Output> {
    let manager = ManagerProxy::new(&session.connection)
        .await
        .context("connect to projd")?;
    match command {
        ProjectTaskCommand::List(args) => list(&manager, args, resolved).await,
        ProjectTaskCommand::Add(args) => create(&manager, args, false, resolved).await,
        ProjectTaskCommand::Set(args) => create(&manager, args, true, resolved).await,
        ProjectTaskCommand::Update(args) => update(&manager, args).await,
        ProjectTaskCommand::Show(args) => show(&manager, args, session, resolved).await,
        ProjectTaskCommand::Status(args) => set_status(&manager, args).await,
        ProjectTaskCommand::Complete(args) => {
            set_goal_status(&manager, args, "completed", false).await
        }
        ProjectTaskCommand::Cancel(args) => {
            set_goal_status(&manager, args, "cancelled", false).await
        }
        ProjectTaskCommand::Start(args) => {
            set_goal_status(&manager, args, "in-progress", true).await
        }
        ProjectTaskCommand::Active(args) => {
            active(&manager, &session.connection, args, resolved).await
        }
        ProjectTaskCommand::Note { command } => match command {
            TaskNoteCommand::Add(args) => super::task_note::add(args, session, resolved).await,
            TaskNoteCommand::List(args) => super::task_note::list(args, session, resolved).await,
            TaskNoteCommand::Show(args) => super::task_note::show(args, session, resolved).await,
            TaskNoteCommand::Path(args) => super::task_note::path(args, session, resolved).await,
            TaskNoteCommand::Check(args) => super::task_note::check(args, session, resolved).await,
            TaskNoteCommand::Delete(args) => {
                super::task_note::delete(args, session, resolved).await
            }
        },
        ProjectTaskCommand::Checkpoint(args) => checkpoint(&manager, args, resolved).await,
        ProjectTaskCommand::Uncheckpoint => uncheckpoint(resolved),
        ProjectTaskCommand::Remove(args) => remove(&manager, args).await,
    }
}

async fn rows(manager: &ManagerProxy<'_>) -> Result<Vec<GoalInfo>> {
    Ok(manager.list_goals("").await?)
}

fn find<'a>(rows: &'a [GoalInfo], id: &str, date: Option<&str>) -> Result<&'a GoalInfo> {
    let matches: Vec<_> = rows
        .iter()
        .filter(|goal| (goal.key() == id || goal.id == id) && date.is_none_or(|d| goal.date == d))
        .collect();
    if matches.len() == 1 {
        Ok(matches[0])
    } else {
        Err(
            CliError::not_found("task not found or ID is ambiguous; use its full date/ID key")
                .into(),
        )
    }
}

async fn list(
    manager: &ManagerProxy<'_>,
    args: TaskListArgs,
    resolved: &Resolved,
) -> Result<Output> {
    let mut rows = rows(manager).await?;
    if let Some(date) = &args.date {
        rows.retain(|goal| &goal.date == date);
    } else if !args.all {
        let project = resolved
            .project
            .as_ref()
            .ok_or_else(|| CliError::invalid("task list requires --project or --all"))?;
        rows.retain(|goal| goal.project_id == project.id);
    }
    let tasks = rows
        .into_iter()
        .map(Goal::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(CliError::invalid)?;
    let text = tasks
        .iter()
        .map(|goal| {
            format!(
                "{} [{}] {}{}",
                goal.key(),
                status_name(&goal.status),
                if goal.parent_id.is_empty() {
                    ""
                } else {
                    "  ↳ "
                },
                goal.title
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Output {
        command: "task.list",
        data: serde_json::to_value(&tasks)?,
        page: None,
        text,
    })
}

fn status_name(status: &proj_model::Status) -> &'static str {
    match status {
        proj_model::Status::Planned => "planned",
        proj_model::Status::InProgress => "in-progress",
        proj_model::Status::Completed => "completed",
        proj_model::Status::Deferred => "deferred",
        proj_model::Status::Cancelled => "cancelled",
    }
}

async fn create(
    manager: &ManagerProxy<'_>,
    args: TaskCreateArgs,
    upsert: bool,
    resolved: &Resolved,
) -> Result<Output> {
    let all = rows(manager).await?;
    if upsert {
        let id = args
            .id
            .as_deref()
            .ok_or_else(|| CliError::invalid("task set requires a stable ID"))?;
        if all.iter().any(|goal| {
            (goal.id == id || goal.key() == id)
                && args.date.as_ref().is_none_or(|d| &goal.date == d)
        }) {
            let existing = find(&all, id, args.date.as_deref())?;
            if args.active && (existing.project_id.is_empty() || !existing.is_open()) {
                bail!("only an open task in a registered project can be tracked");
            }
            let mut fields = HashMap::from([("title".to_string(), args.title)]);
            if let Some(description) = args.description {
                fields.insert("description".to_string(), description);
            }
            let result: GoalInfo = manager.edit_goal(&existing.key(), fields).await?;
            if args.active {
                select(manager, &result).await?;
            }
            return written("task.set", &result);
        }
    }
    let parent = match args.parent.as_deref() {
        Some(id) => Some(find(&all, id, None)?.clone()),
        None => None,
    };
    let project = if let Some(parent) = &parent {
        parent.project.clone()
    } else {
        resolved
            .project
            .as_ref()
            .map(|project| project.cwd.clone())
            .unwrap_or_default()
    };
    let goal = GoalInfo {
        id: args
            .id
            .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string()),
        date: args.date.unwrap_or_default(),
        title: args.title,
        description: args.description.unwrap_or_default(),
        parent_id: parent.map(|parent| parent.key()).unwrap_or_default(),
        project,
        kind: args.kind,
        success: args.success,
        priority: args.priority,
        status: "planned".into(),
        ..Default::default()
    };
    if args.active {
        if goal.project.is_empty() {
            bail!("an active task requires a registered project");
        }
        let _: ProjectInfo = manager.resolve_project(&goal.project).await?;
    }
    let result: GoalInfo = manager.create_goal(&goal).await?;
    if args.active {
        select(manager, &result).await?;
    }
    written("task.add", &result)
}

async fn update(manager: &ManagerProxy<'_>, args: TaskUpdateArgs) -> Result<Output> {
    let all = rows(manager).await?;
    let goal = find(&all, &args.id, args.date.as_deref())?;
    let mut fields = HashMap::new();
    if let Some(title) = args.title {
        fields.insert("title".to_string(), title);
    }
    if let Some(description) = args.description {
        fields.insert("description".to_string(), description);
    }
    let result: GoalInfo = manager.edit_goal(&goal.key(), fields).await?;
    written("task.update", &result)
}

async fn show(
    manager: &ManagerProxy<'_>,
    args: TaskRefArgs,
    session: &Session,
    resolved: &Resolved,
) -> Result<Output> {
    let all = rows(manager).await?;
    let goal: Goal = find(&all, &args.id, args.date.as_deref())?
        .clone()
        .try_into()
        .map_err(CliError::invalid)?;
    let key = goal.key();
    let parent = all
        .iter()
        .find(|candidate| candidate.key() == goal.parent_id)
        .map(|candidate| candidate.key());
    let children = all
        .iter()
        .filter(|candidate| candidate.parent_id == key)
        .map(|candidate| {
            serde_json::json!({
                "key": candidate.key(),
                "title": candidate.title,
                "status": candidate.status,
            })
        })
        .collect::<Vec<_>>();
    let instance_id = task_instance(session, &key).await;
    let notes = match instance_id {
        Some(instance_id) => attachable_notes(session, resolved, &instance_id).await,
        None => Vec::new(),
    };
    let text = format!(
        "{}\nnotes: {}\nchildren: {}",
        serde_json::to_string_pretty(&goal)?,
        notes.len(),
        children.len()
    );
    Ok(Output {
        command: "task.show",
        data: serde_json::json!({
            "task": goal,
            "parent": parent,
            "children": children,
            "notes": notes,
        }),
        page: None,
        text,
    })
}

async fn task_instance(session: &Session, key: &str) -> Option<String> {
    let metrics = MetricsProxy::new(&session.connection).await.ok()?;
    let rows = metrics.list_goal_identities(key).await.ok()?;
    rows.into_iter()
        .find(|row| row.key == key && row.deleted_at_ms == 0)
        .map(|row| row.instance_id)
}

async fn attachable_notes(
    session: &Session,
    resolved: &Resolved,
    instance_id: &str,
) -> Vec<serde_json::Value> {
    if resolved.project.is_none() {
        return Vec::new();
    }
    let Ok(notebook) = session.notes().await else {
        return Vec::new();
    };
    let Ok(notes) = notebook
        .query_notes(notebook_api::note::NoteQuery {
            required_tags: vec![task_notes::task_tag(instance_id)],
            limit: 500,
            ..Default::default()
        })
        .await
    else {
        return Vec::new();
    };
    notes
        .notes
        .into_iter()
        .map(|note| {
            serde_json::json!({
                "note_id": note.id,
                "title": note.title,
                "kind": note.metadata.kind,
                "tags": note.tags,
                "modified": note.modified,
            })
        })
        .collect()
}

async fn set_status(manager: &ManagerProxy<'_>, args: TaskStatusArgs) -> Result<Output> {
    let status = args.status.as_str();
    set_goal_status(
        manager,
        TaskRefArgs {
            id: args.id,
            date: args.date,
        },
        status,
        false,
    )
    .await
}

async fn set_goal_status(
    manager: &ManagerProxy<'_>,
    args: TaskRefArgs,
    status: &str,
    active: bool,
) -> Result<Output> {
    let all = rows(manager).await?;
    let goal = find(&all, &args.id, args.date.as_deref())?;
    if active && goal.project_id.is_empty() {
        bail!("task has no registered project");
    }
    let result: GoalInfo = manager.set_goal_status(&goal.key(), status).await?;
    if active {
        select(manager, &result).await?;
    }
    written("task.status", &result)
}

async fn active(
    manager: &ManagerProxy<'_>,
    connection: &zbus::Connection,
    args: TaskActiveArgs,
    resolved: &Resolved,
) -> Result<Output> {
    if args.clear && args.id.is_some() {
        bail!("use a task ID or --clear");
    }
    if let Some(id) = args.id {
        let all = rows(manager).await?;
        let goal = find(&all, &id, None)?;
        if goal.project_id.is_empty() {
            bail!("task has no registered project");
        }
        let _: () = manager
            .set_active_goal(&goal.project_id, &goal.key())
            .await?;
        return Ok(Output {
            command: "task.active",
            data: serde_json::to_value(goal)?,
            page: None,
            text: goal.key(),
        });
    }
    let project = resolved.project.clone().ok_or_else(|| {
        CliError::invalid("task active requires --project or a registered current directory")
    })?;
    if args.clear {
        let _: () = manager.set_active_goal(&project.id, "").await?;
        return Ok(Output {
            command: "task.active",
            data: serde_json::json!({ "project_id": project.id, "active": null }),
            page: None,
            text: "null".to_string(),
        });
    }
    let object = zbus::Proxy::new(
        connection,
        proj_model::BUS_NAME,
        proj_model::object_path("Projects", &project.id),
        proj_model::PROJECT_INTERFACE,
    )
    .await?;
    let active: zbus::zvariant::OwnedObjectPath = object.get_property("ActiveGoal").await?;
    let all = rows(manager).await?;
    let goal = all
        .iter()
        .find(|goal| goal.object_path() == active.as_str());
    Ok(Output {
        command: "task.active",
        data: serde_json::json!({ "project_id": project.id, "active": goal }),
        page: None,
        text: goal
            .map(|goal| serde_json::to_string(goal).unwrap_or_default())
            .unwrap_or_else(|| "null".to_string()),
    })
}

async fn checkpoint(
    manager: &ManagerProxy<'_>,
    args: TaskCheckpointArgs,
    resolved: &Resolved,
) -> Result<Output> {
    let project = project(resolved)?;
    let all = rows(manager).await?;
    let goal = find(&all, &args.task, None)?;
    if goal.project_id != project.id {
        bail!(
            "task {} does not belong to project {}",
            goal.key(),
            project.name
        );
    }
    let path = crate::scope::write_checkpoint(&project.cwd, &goal.key())?;
    Ok(Output {
        command: "task.checkpoint",
        data: serde_json::json!({ "task": goal.key(), "checkpoint": path }),
        page: None,
        text: goal.key(),
    })
}

fn uncheckpoint(resolved: &Resolved) -> Result<Output> {
    let project = project(resolved)?;
    let path = crate::scope::clear_checkpoint(&project.cwd)?;
    Ok(Output {
        command: "task.uncheckpoint",
        data: serde_json::json!({ "checkpoint": path }),
        page: None,
        text: path.display().to_string(),
    })
}

fn project(resolved: &Resolved) -> Result<&ProjectInfo> {
    resolved
        .project
        .as_ref()
        .ok_or_else(|| CliError::invalid("task checkpoint requires a registered project").into())
}

async fn remove(manager: &ManagerProxy<'_>, args: TaskRefArgs) -> Result<Output> {
    let all = rows(manager).await?;
    let goal = find(&all, &args.id, args.date.as_deref())?;
    let removed: bool = manager.remove_goal(&goal.date, &goal.id).await?;
    if !removed {
        return Err(CliError::not_found("task not found").into());
    }
    Ok(Output {
        command: "task.remove",
        data: serde_json::json!({ "task": goal.key(), "removed": true }),
        page: None,
        text: goal.key(),
    })
}

async fn select(manager: &ManagerProxy<'_>, goal: &GoalInfo) -> Result<()> {
    if goal.project_id.is_empty() {
        bail!("task has no registered project");
    }
    let _: () = manager
        .set_active_goal(&goal.project_id, &goal.key())
        .await?;
    Ok(())
}

fn written(command: &'static str, goal: &GoalInfo) -> Result<Output> {
    Ok(Output {
        command,
        data: serde_json::to_value(goal)?,
        page: None,
        text: goal.key(),
    })
}
