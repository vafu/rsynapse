use anyhow::Result;
use notebook_api::note::{NoteCapture, NoteId, NoteKind, NoteQuery, NoteRevision};
use serde_json::{Value, json};

use crate::Session;
use crate::args::{LinkDirection, NoteCommand};
use crate::output::{CliError, Output, PageInfo};
use crate::scope::Resolved;

use super::common::{find_note, read_primary, validate_limit};
use super::markdown_task::task_records_value;

pub async fn execute(
    command: NoteCommand,
    session: &Session,
    resolved: &Resolved,
) -> Result<Output> {
    match command {
        NoteCommand::Capture(args) => capture(args, session, resolved).await,
        NoteCommand::List(args) => list(args, session).await,
        NoteCommand::Get(args) => get(args, session).await,
        NoteCommand::Context(args) => context(args.note_id, session).await,
        NoteCommand::Replace(args) => replace(args, session).await,
        NoteCommand::Delete(args) => delete(args, session).await,
        NoteCommand::Links(args) => links(args.note_id, args.direction, session).await,
    }
}

async fn capture(
    args: crate::args::NoteCaptureArgs,
    session: &Session,
    resolved: &Resolved,
) -> Result<Output> {
    let body = read_primary(args.body, "note body")?;
    // Scope tags are applied by the service; explicit capture tags must not
    // duplicate the task tag the wrapper already injected.
    let mut tags = args.tags;
    if let Some(task) = &resolved.task {
        let tag = task_notes::task_tag(&task.instance_id);
        tags.retain(|existing| existing != &tag);
    }
    let note_id = session
        .notes()
        .await?
        .capture_note(NoteCapture { body, tags })
        .await?;
    let note = find_note(session, &note_id).await?;
    let body = session.notes().await?.note_body(&note_id).await?;
    Ok(Output {
        command: "note.capture",
        data: json!({ "note": note, "revision": body.revision }),
        page: None,
        text: note_id.to_string(),
    })
}

async fn list(args: crate::args::NoteListArgs, session: &Session) -> Result<Output> {
    let limit = validate_limit(args.limit)?;
    let mut tags = args.tags;
    if args.inbox {
        tags.push("status/inbox".to_string());
    }
    let kind = args
        .kind
        .map(|kind| kind.parse::<NoteKind>())
        .transpose()
        .map_err(CliError::invalid)?;
    let page = session
        .notes()
        .await?
        .query_notes(NoteQuery {
            required_tags: tags,
            kind: kind.into_iter().collect(),
            limit,
            ..Default::default()
        })
        .await?;
    let data = if args.ids {
        Value::Array(page.notes.iter().map(|note| json!(note.id)).collect())
    } else {
        serde_json::to_value(&page.notes)?
    };
    let text = page
        .notes
        .iter()
        .map(|note| {
            if args.ids {
                note.id.to_string()
            } else {
                format!("{}\t{}", note.id, note.title)
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Output {
        command: "note.list",
        data,
        page: Some(PageInfo {
            limit: page.limit,
            truncated: page.truncated,
        }),
        text,
    })
}

async fn get(args: crate::args::NoteGetArgs, session: &Session) -> Result<Output> {
    let mut records = Vec::new();
    let mut bodies = Vec::new();
    for value in args.note_ids {
        let note_id = NoteId::new(value);
        let note = find_note(session, &note_id).await?;
        let body = session.notes().await?.note_body(&note_id).await?;
        bodies.push(body.text.clone());
        records.push(json!({ "note": note, "body": body.text, "revision": body.revision }));
    }
    let text = if args.body {
        bodies.join("\n\n---\n\n")
    } else {
        records
            .iter()
            .map(|record| {
                format!(
                    "{}\trevision {}",
                    record["note"]["id"].as_str().unwrap_or_default(),
                    record["revision"].as_str().unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(Output {
        command: "note.get",
        data: Value::Array(records),
        page: None,
        text,
    })
}

async fn context(note_id: String, session: &Session) -> Result<Output> {
    let note_id = NoteId::new(note_id);
    let note = find_note(session, &note_id).await?;
    let body = session.notes().await?.note_body(&note_id).await?;
    let links = session.notes().await?.linked_notes(&note_id).await?;
    let backlinks = session.notes().await?.backlinked_notes(&note_id).await?;
    let tasks = session.notes().await?.list_note_tasks(&note_id).await?;
    let task_records = task_records_value(&tasks)?;
    let proposal = session.notes().await?.proposal_for_note(&note_id).await?;
    let text = format!(
        "{}\nrevision: {}\nlinks: {} out, {} in\ntasks: {}\n\n{}",
        note.id,
        body.revision,
        links.len(),
        backlinks.len(),
        tasks.len(),
        body.text
    );
    Ok(Output {
        command: "note.context",
        data: json!({
            "note": note,
            "body": body.text,
            "revision": body.revision,
            "links": links,
            "backlinks": backlinks,
            "tasks": task_records,
            "triage_proposal": proposal,
        }),
        page: None,
        text,
    })
}

async fn replace(args: crate::args::NoteReplaceArgs, session: &Session) -> Result<Output> {
    let body = read_primary(args.body, "note body")?;
    let note_id = NoteId::new(args.note_id);
    let result = session
        .notes()
        .await?
        .replace_note(&note_id, &NoteRevision::new(args.revision), &body)
        .await?;
    let text = format!("{}\trevision {}", result.message, result.revision);
    Ok(Output {
        command: "note.replace",
        data: serde_json::to_value(result)?,
        page: None,
        text,
    })
}

async fn delete(args: crate::args::NoteDeleteArgs, session: &Session) -> Result<Output> {
    if !args.yes {
        return Err(CliError::rejected("note delete requires --yes").into());
    }
    let note_id = NoteId::new(args.note_id);
    let result = session
        .notes()
        .await?
        .delete_note(&note_id, &NoteRevision::new(args.revision))
        .await?;
    Ok(Output {
        command: "note.delete",
        data: json!({ "note_id": note_id, "result": result }),
        page: None,
        text: "note deleted".to_string(),
    })
}

async fn links(note_id: String, direction: LinkDirection, session: &Session) -> Result<Output> {
    let note_id = NoteId::new(note_id);
    find_note(session, &note_id).await?;
    let outgoing = if matches!(direction, LinkDirection::Out | LinkDirection::Both) {
        session.notes().await?.linked_notes(&note_id).await?
    } else {
        Vec::new()
    };
    let incoming = if matches!(direction, LinkDirection::In | LinkDirection::Both) {
        session.notes().await?.backlinked_notes(&note_id).await?
    } else {
        Vec::new()
    };
    let text = outgoing
        .iter()
        .map(|note| format!("out\t{}\t{}", note.id, note.title))
        .chain(
            incoming
                .iter()
                .map(|note| format!("in\t{}\t{}", note.id, note.title)),
        )
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Output {
        command: "note.links",
        data: json!({ "note_id": note_id, "outgoing": outgoing, "incoming": incoming }),
        page: None,
        text,
    })
}

pub async fn tags(session: &Session) -> Result<Output> {
    let tags = session.notes().await?.list_tags().await?;
    Ok(Output {
        command: "tags.list",
        data: serde_json::to_value(&tags)?,
        page: None,
        text: tags.join("\n"),
    })
}
