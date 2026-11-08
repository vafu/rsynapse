//! Task-attached Remarked notes.
//!
//! Notes are created through the Remarked service, then normalized on disk:
//! moved to `tasks/`, given canonical frontmatter, and stripped of
//! `status/inbox`. The printed path is the edit interface.
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use notebook_api::note::{Note, NoteCapture, NoteId, NoteQuery, NoteRevision};
use proj_model::GoalIdentityInfo;
use serde_json::json;

use crate::Session;
use crate::args::{
    TaskNoteAddArgs, TaskNoteCheckArgs, TaskNoteDeleteArgs, TaskNoteListArgs, TaskNotePathArgs,
    TaskNoteShowArgs,
};
use crate::output::{CliError, Output, PageInfo};
use crate::scope::Resolved;
use crate::tasknotes::{self, frontmatter};

use super::common::read_primary_preserving_newline;

pub async fn add(args: TaskNoteAddArgs, session: &Session, resolved: &Resolved) -> Result<Output> {
    let task = resolved.task_required()?.clone();
    let notebook = notebook_dir(resolved)?;
    let role = tasknotes::parse_role(&args.role).map_err(CliError::invalid)?;
    if !args.new
        && tasknotes::is_main_role(&role)
        && let Some(existing) = find_role(session, resolved, &task, &role).await?
    {
        let path = absolute_note_path(&notebook, &existing)?;
        eprintln!("main note already exists: {}", path.display());
        return Ok(Output {
            command: "task.note.add",
            data: json!({ "note": existing, "path": path, "created": false }),
            page: None,
            text: path.display().to_string(),
        });
    }

    let body = match args.body {
        Some(body) => read_primary_preserving_newline(Some(body), "note body", false)?,
        None => default_body(&task, &role),
    };
    let note_id = session
        .notes()
        .await?
        .capture_note(NoteCapture {
            body: body.clone(),
            tags: Vec::new(),
        })
        .await?;

    let source = notebook.join(note_id.as_str());
    let file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .context("note id has no file name")?
        .to_string();
    let destination = tasknotes::destination_path(&notebook, &file_name);
    std::fs::create_dir_all(destination.parent().expect("tasks dir has parent"))
        .with_context(|| format!("create {}", destination.display()))?;
    std::fs::rename(&source, &destination)
        .with_context(|| format!("move {} to {}", source.display(), destination.display()))?;

    let mapping = tasknotes::NewNote {
        instance_id: task.instance_id.clone(),
        role: role.clone(),
        kind: args.kind,
        title: args
            .title
            .or_else(|| (role == tasknotes::MAIN_ROLE).then(|| task.goal.title.clone())),
        extra_tags: args.tags,
    }
    .mapping();
    // The create-time body is already in the document; rewrite only the
    // frontmatter around it so the agent's text is untouched.
    let document = frontmatter::read(&destination)?;
    tasknotes::atomic_write(&destination, &tasknotes::render(&mapping, &document.body)?)?;

    let note_id = NoteId::new(relative(&notebook, &destination));
    let note = find_note(session, &note_id).await?;
    Ok(Output {
        command: "task.note.add",
        data: json!({ "note": note, "path": destination, "created": true }),
        page: None,
        text: destination.display().to_string(),
    })
}

pub async fn list(
    args: TaskNoteListArgs,
    session: &Session,
    resolved: &Resolved,
) -> Result<Output> {
    let task = resolved.task_required()?.clone();
    let mut notes = notes_for(session, resolved, &task).await?;
    if let Some(role) = &args.role {
        notes.retain(|(_, role_tag)| role_tag.as_deref() == Some(role.as_str()));
    }
    let records = notes
        .iter()
        .map(|(note, role)| {
            json!({
                "note_id": note.id,
                "title": note.title,
                "role": role,
                "kind": note.metadata.kind,
                "modified": note.modified,
                "tags": note.tags,
            })
        })
        .collect::<Vec<_>>();
    let text = notes
        .iter()
        .map(|(note, role)| {
            format!(
                "{}\t{}\t{}\t{}",
                note.id,
                role.as_deref().unwrap_or("-"),
                note.metadata.kind,
                note.title
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Output {
        command: "task.note.list",
        data: serde_json::Value::Array(records),
        page: Some(PageInfo {
            limit: notes.len() as u64,
            truncated: false,
        }),
        text,
    })
}

pub async fn show(
    args: TaskNoteShowArgs,
    session: &Session,
    resolved: &Resolved,
) -> Result<Output> {
    let task = resolved.task_required()?.clone();
    let note_id = resolve_note_id(&args.note_id);
    ensure_attached(session, resolved, &task, &note_id).await?;
    let note = find_note(session, &note_id).await?;
    let body = session.notes().await?.note_body(&note_id).await?;
    let path = absolute_note_path(&notebook_dir(resolved)?, &note)?;
    Ok(Output {
        command: "task.note.show",
        data: json!({
            "note": note,
            "path": path,
            "revision": body.revision,
            "body": body.text,
        }),
        page: None,
        text: format!(
            "{}\nrevision: {}\ntags: {}\n\n{}",
            path.display(),
            body.revision,
            note.tags.join(", "),
            body.text
        ),
    })
}

pub async fn path(
    args: TaskNotePathArgs,
    session: &Session,
    resolved: &Resolved,
) -> Result<Output> {
    let task = resolved.task_required()?.clone();
    let notebook = notebook_dir(resolved)?;
    let notes = notes_for(session, resolved, &task).await?;
    let note = match args.role.as_deref() {
        Some(role) => notes
            .iter()
            .find(|(_, note_role)| note_role.as_deref() == Some(role))
            .map(|(note, _)| note),
        None => notes
            .iter()
            .find(|(_, note_role)| note_role.as_deref() == Some(tasknotes::MAIN_ROLE))
            .or_else(|| notes.first())
            .map(|(note, _)| note),
    }
    .ok_or_else(|| CliError::not_found("no note attached to this task"))?;
    let path = absolute_note_path(&notebook, note)?;
    Ok(Output {
        command: "task.note.path",
        data: json!({ "note_id": note.id, "path": path }),
        page: None,
        text: path.display().to_string(),
    })
}

pub async fn check(
    args: TaskNoteCheckArgs,
    _session: &Session,
    resolved: &Resolved,
) -> Result<Output> {
    let task = resolved.task_required()?.clone();
    let notebook = notebook_dir(resolved)?;
    let path = resolve_target(&notebook, &args.target)?;
    let document = frontmatter::read(&path)?;
    let mut mapping = tasknotes::parse(&document).map_err(|error| {
        CliError::new("invalid-input", 3, format!("{}: {error}", path.display()))
    })?;
    if args.fix {
        let before = mapping.clone();
        tasknotes::fix(&mut mapping, &task.instance_id, None).map_err(CliError::invalid)?;
        if mapping != before {
            tasknotes::atomic_write(&path, &tasknotes::render(&mapping, &document.body)?)?;
        }
    }
    let report = tasknotes::check(&mapping, &task.instance_id);
    let title = tasknotes::frontmatter::title(&mapping);
    let text = if report.is_clean() {
        format!("{}: ok", path.display())
    } else {
        report
            .findings
            .iter()
            .map(|finding| {
                let level = match finding.severity {
                    tasknotes::Severity::Error => "error",
                    tasknotes::Severity::Warning => "warning",
                };
                format!("{}: {level}: {}", path.display(), finding.message)
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    if report.errors() > 0 {
        return Err(CliError::new("invalid-input", 3, text).into());
    }
    Ok(Output {
        command: "task.note.check",
        data: json!({
            "path": path,
            "title": title,
            "errors": report.errors(),
            "warnings": report.warnings(),
            "findings": report.findings.iter().map(|finding| json!({
                "severity": match finding.severity {
                    tasknotes::Severity::Error => "error",
                    tasknotes::Severity::Warning => "warning",
                },
                "message": finding.message,
            })).collect::<Vec<_>>(),
        }),
        page: None,
        text,
    })
}

pub async fn delete(
    args: TaskNoteDeleteArgs,
    session: &Session,
    resolved: &Resolved,
) -> Result<Output> {
    if !args.yes {
        return Err(CliError::rejected("note delete requires --yes").into());
    }
    let task = resolved.task_required()?.clone();
    let note_id = resolve_note_id(&args.note_id);
    ensure_attached(session, resolved, &task, &note_id).await?;
    let body = session.notes().await?.note_body(&note_id).await?;
    let revision = NoteRevision::new(body.revision.as_str());
    session
        .notes()
        .await?
        .delete_note(&note_id, &revision)
        .await?;
    Ok(Output {
        command: "task.note.delete",
        data: json!({ "note_id": note_id, "deleted": true }),
        page: None,
        text: "note deleted".to_string(),
    })
}

async fn notes_for(
    session: &Session,
    resolved: &Resolved,
    task: &GoalIdentityInfo,
) -> Result<Vec<(Note, Option<String>)>> {
    let page = session
        .notes()
        .await?
        .query_notes(NoteQuery {
            required_tags: vec![task_notes::task_tag(&task.instance_id)],
            limit: 500,
            ..Default::default()
        })
        .await?;
    let notebook = notebook_dir(resolved)?;
    let mut notes = Vec::new();
    for note in page.notes {
        let path = absolute_note_path(&notebook, &note)?;
        let role = match frontmatter::read(&path) {
            Ok(document) => tasknotes::parse(&document)
                .ok()
                .and_then(|mapping| tasknotes::role_of(&mapping)),
            Err(_) => None,
        };
        notes.push((note, role));
    }
    notes.sort_by(|(a, _), (b, _)| a.created.cmp(&b.created));
    Ok(notes)
}

async fn find_role(
    session: &Session,
    resolved: &Resolved,
    task: &GoalIdentityInfo,
    role: &str,
) -> Result<Option<Note>> {
    Ok(notes_for(session, resolved, task)
        .await?
        .into_iter()
        .find(|(_, note_role)| note_role.as_deref() == Some(role))
        .map(|(note, _)| note))
}

async fn find_note(session: &Session, note_id: &NoteId) -> Result<Note> {
    let page = session
        .notes()
        .await?
        .query_notes(NoteQuery {
            note_ids: vec![note_id.clone()],
            limit: 1,
            ..Default::default()
        })
        .await?;
    page.notes.into_iter().next().ok_or_else(|| {
        CliError::not_found(format!("note not found: {note_id}"))
            .detail("note_id", note_id.to_string())
            .into()
    })
}

/// Accepts a full notebook-relative id (`tasks/abc.md`) or a bare file name
/// (`abc.md`), since printed paths and listings use both forms.
fn resolve_note_id(value: &str) -> NoteId {
    let value = value.trim();
    if value.contains('/') {
        NoteId::new(value)
    } else {
        NoteId::new(format!("{}/{}", tasknotes::TASKS_DIR, value))
    }
}

async fn ensure_attached(
    session: &Session,
    resolved: &Resolved,
    task: &GoalIdentityInfo,
    note_id: &NoteId,
) -> Result<()> {
    let attached = notes_for(session, resolved, task).await?;
    if attached.iter().any(|(note, _)| &note.id == note_id) {
        Ok(())
    } else {
        Err(CliError::not_found(format!(
            "note {note_id} is not attached to task {}",
            task.key
        ))
        .into())
    }
}

fn notebook_dir(resolved: &Resolved) -> Result<PathBuf> {
    resolved
        .scope
        .storage_path()
        .map(PathBuf::from)
        .ok_or_else(|| CliError::invalid("task notes require a project notebook").into())
}

fn relative(notebook: &Path, path: &Path) -> String {
    path.strip_prefix(notebook)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn absolute_note_path(notebook: &Path, note: &Note) -> Result<PathBuf> {
    let path = notebook.join(&note.path);
    if path.exists() {
        return Ok(path);
    }
    if note.path.starts_with(tasknotes::TASKS_DIR) {
        return Ok(path);
    }
    let moved = tasknotes::destination_path(
        notebook,
        Path::new(&note.path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(note.path.as_str()),
    );
    if moved.exists() {
        bail!("note {} was moved to {}", note.id, moved.display());
    }
    Ok(path)
}

fn resolve_target(notebook: &Path, target: &str) -> Result<PathBuf> {
    let path = Path::new(target);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let joined = notebook.join(path);
    if joined.exists() {
        return Ok(joined);
    }
    let current = std::env::current_dir()?.join(path);
    if current.exists() {
        return Ok(current);
    }
    Ok(joined)
}

fn default_body(task: &GoalIdentityInfo, role: &str) -> String {
    let title = if task.goal.title.trim().is_empty() {
        "Untitled"
    } else {
        task.goal.title.trim()
    };
    let description = task.goal.description.trim();
    if role == tasknotes::MAIN_ROLE && !description.is_empty() {
        format!("# {title}\n\n{description}\n")
    } else {
        format!("# {title}\n")
    }
}
