use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use notebook_api::note::{NoteId, NoteRevision};
use notebook_api::task::{TaskItem, TaskLocator, TaskStatus};
use serde::Serialize;
use serde_json::{Value, json};

use crate::Session;
use crate::args::{MarkdownTaskCommand, MarkdownTaskFilter, MarkdownTaskStatusValue};
use crate::output::{CliError, Output, PageInfo};
use crate::scope::Resolved;

use super::common::{read_primary, validate_limit};

#[derive(Serialize)]
struct TaskRecord<'a> {
    task_ref: String,
    note_id: &'a NoteId,
    revision: &'a NoteRevision,
    line: u64,
    detail_note_id: Option<&'a NoteId>,
    parent_ref: Option<String>,
    text: &'a str,
    status: &'static str,
}

pub async fn execute(
    command: MarkdownTaskCommand,
    session: &Session,
    _resolved: &Resolved,
) -> Result<Output> {
    match command {
        MarkdownTaskCommand::List(args) => list(args, session).await,
        MarkdownTaskCommand::Capture(args) => {
            let text = read_primary(args.text, "task text")?;
            let task = session.notes().await?.capture_task(&text).await?;
            task_output("markdown-task.capture", task)
        }
        MarkdownTaskCommand::Append(args) => {
            let text = read_primary(args.text, "task text")?;
            let task = session
                .notes()
                .await?
                .append_task(
                    &NoteId::new(args.note_id),
                    &NoteRevision::new(args.revision),
                    &text,
                )
                .await?;
            task_output("markdown-task.append", task)
        }
        MarkdownTaskCommand::Subtask(args) => {
            let text = read_primary(args.text, "task text")?;
            let locator = decode_ref(&args.task_ref)?;
            let parent = session.notes().await?.resolve_task(&locator).await?;
            let task = session
                .notes()
                .await?
                .create_subtask(&parent.locator, &text)
                .await?;
            task_output("markdown-task.subtask", task)
        }
        MarkdownTaskCommand::SetStatus(args) => {
            let status = match args.status {
                MarkdownTaskStatusValue::Open => TaskStatus::Open,
                MarkdownTaskStatusValue::Completed => TaskStatus::Completed,
            };
            let locator = decode_ref(&args.task_ref)?;
            let task = session.notes().await?.resolve_task(&locator).await?;
            let task = session
                .notes()
                .await?
                .set_task_status(&task.locator, status)
                .await?;
            task_output("markdown-task.set-status", task)
        }
        MarkdownTaskCommand::Complete(args) => {
            set_many(
                args.task_refs,
                TaskStatus::Completed,
                "markdown-task.complete",
                session,
            )
            .await
        }
        MarkdownTaskCommand::Reopen(args) => {
            set_many(
                args.task_refs,
                TaskStatus::Open,
                "markdown-task.reopen",
                session,
            )
            .await
        }
    }
}

async fn list(args: crate::args::MarkdownTaskListArgs, session: &Session) -> Result<Output> {
    let limit = validate_limit(args.limit)?;
    let tasks = if let Some(note_id) = args.note.as_deref() {
        session
            .notes()
            .await?
            .list_note_tasks(&NoteId::new(note_id))
            .await?
    } else {
        session.notes().await?.list_scope_tasks().await?
    };
    let mut tasks = tasks
        .into_iter()
        .filter(|task| match args.status {
            MarkdownTaskFilter::Open => task.status == TaskStatus::Open,
            MarkdownTaskFilter::Completed => task.status == TaskStatus::Completed,
            MarkdownTaskFilter::All => true,
        })
        .filter(|task| !args.roots || task.parent.is_empty())
        .collect::<Vec<_>>();
    let truncated = tasks.len() > limit as usize;
    tasks.truncate(limit as usize);
    let data = if args.ids {
        Value::Array(
            tasks
                .iter()
                .map(|task| json!(encode_ref(&task.locator)))
                .collect(),
        )
    } else {
        serde_json::to_value(tasks.iter().map(task_record).collect::<Vec<_>>())?
    };
    let text = tasks
        .iter()
        .map(|task| {
            if args.ids {
                encode_ref(&task.locator)
            } else {
                format!(
                    "{}\t{}:{}\t{}\t{}",
                    encode_ref(&task.locator),
                    task.locator.note_id,
                    task.locator.line,
                    status_text(task.status),
                    task.text
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Output {
        command: "markdown-task.list",
        data,
        page: Some(PageInfo { limit, truncated }),
        text,
    })
}

async fn set_many(
    references: Vec<String>,
    status: TaskStatus,
    command: &'static str,
    session: &Session,
) -> Result<Output> {
    let mut changed = Vec::new();
    let mut failed = Vec::new();
    for reference in references {
        let result = async {
            let locator = decode_ref(&reference)?;
            let task = session.notes().await?.resolve_task(&locator).await?;
            session
                .notes()
                .await?
                .set_task_status(&task.locator, status)
                .await
        }
        .await;
        match result {
            Ok(task) => changed.push(task),
            Err(error) => failed.push((reference, error)),
        }
    }
    if changed.is_empty() && !failed.is_empty() {
        return Err(failed.remove(0).1);
    }
    if !failed.is_empty() {
        let failures = failed
            .into_iter()
            .map(|(reference, error)| format!("{reference}: {error:#}"))
            .collect::<Vec<_>>();
        return Err(CliError::new(
            "partial-failure",
            8,
            format!("{} task mutations failed", failures.len()),
        )
        .detail("failures", failures.join(" | "))
        .detail(
            "updated_task_refs",
            changed
                .iter()
                .map(|task| encode_ref(&task.locator))
                .collect::<Vec<_>>()
                .join(","),
        )
        .into());
    }
    let records = changed.iter().map(task_record).collect::<Vec<_>>();
    let text = changed
        .iter()
        .map(|task| encode_ref(&task.locator))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Output {
        command,
        data: serde_json::to_value(records)?,
        page: None,
        text,
    })
}

fn task_output(command: &'static str, task: TaskItem) -> Result<Output> {
    let text = encode_ref(&task.locator);
    let data = serde_json::to_value(task_record(&task))?;
    Ok(Output {
        command,
        data,
        page: None,
        text,
    })
}

fn task_record(task: &TaskItem) -> TaskRecord<'_> {
    TaskRecord {
        task_ref: encode_ref(&task.locator),
        note_id: &task.locator.note_id,
        revision: &task.locator.note_revision,
        line: task.locator.line,
        detail_note_id: task.detail_note_id.first(),
        parent_ref: task.parent.first().map(encode_ref),
        text: &task.text,
        status: status_text(task.status),
    }
}

pub(super) fn task_records_value(tasks: &[TaskItem]) -> Result<Value> {
    Ok(serde_json::to_value(
        tasks.iter().map(task_record).collect::<Vec<_>>(),
    )?)
}

fn status_text(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Open => "open",
        TaskStatus::Completed => "completed",
    }
}

fn encode_ref(locator: &TaskLocator) -> String {
    let note_id = URL_SAFE_NO_PAD.encode(locator.note_id.as_str());
    let revision = URL_SAFE_NO_PAD.encode(locator.note_revision.as_str());
    format!("tr1.{note_id}.{revision}.{}", locator.line)
}

fn decode_ref(value: &str) -> Result<TaskLocator> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != 4 || parts[0] != "tr1" {
        return Err(CliError::invalid("invalid task reference").into());
    }
    let note_id = decode_text(parts[1])?;
    let revision = decode_text(parts[2])?;
    let line = parts[3]
        .parse::<u64>()
        .map_err(|_| CliError::invalid("decode task source line"))?;
    if line == 0 {
        return Err(CliError::invalid("task source line must be one-based").into());
    }
    Ok(TaskLocator {
        note_id: NoteId::new(note_id),
        note_revision: NoteRevision::new(revision),
        line,
    })
}

fn decode_text(value: &str) -> Result<String> {
    String::from_utf8(URL_SAFE_NO_PAD.decode(value)?)
        .map_err(|_| CliError::invalid("task reference contains invalid text").into())
}

#[cfg(test)]
mod test;
