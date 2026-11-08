use anyhow::Result;
use notebook_api::note::NoteId;
use notebook_api::triage::TriageProposal;
use serde_json::{Value, json};

use crate::Session;
use crate::args::TriageCommand;
use crate::output::{CliError, Output};

use super::common::find_note;

pub async fn execute(command: TriageCommand, session: &Session) -> Result<Output> {
    match command {
        TriageCommand::Policy => policy(session).await,
        TriageCommand::Process(args) => process(args.note_id, session).await,
        TriageCommand::ProcessInbox => process_inbox(session).await,
        TriageCommand::List(args) => list(args.ids, session).await,
        TriageCommand::Show(args) => show(args.note_id, session).await,
        TriageCommand::Commit(args) => commit(args, session).await,
    }
}

async fn policy(session: &Session) -> Result<Output> {
    let policy = Session::triage_policy(&session.connection).await?;
    let text = format!(
        "inbox: {}\ntriaged: {}\ndefault kind: {}\nkinds: {}",
        policy.inbox_status,
        policy.triaged_status,
        policy.default_kind,
        policy.kinds.join(", ")
    );
    Ok(Output {
        command: "triage.policy",
        data: serde_json::to_value(policy)?,
        page: None,
        text,
    })
}

async fn process(note_id: String, session: &Session) -> Result<Output> {
    let note_id = NoteId::new(note_id);
    find_note(session, &note_id).await?;
    let changed = session.notes().await?.process_note(&note_id).await?;
    let proposal = session.notes().await?.proposal_for_note(&note_id).await?;
    Ok(Output {
        command: "triage.process",
        data: json!({ "changed": changed, "proposal": proposal }),
        page: None,
        text: if changed {
            "triage proposal updated".to_string()
        } else {
            "note skipped".to_string()
        },
    })
}

async fn process_inbox(session: &Session) -> Result<Output> {
    let count = session.notes().await?.process_inbox().await?;
    Ok(Output {
        command: "triage.process-inbox",
        data: json!({ "processed": count }),
        page: None,
        text: format!("{count} inbox notes processed"),
    })
}

async fn list(ids: bool, session: &Session) -> Result<Output> {
    let proposals = session.notes().await?.list_proposals().await?;
    let data = if ids {
        Value::Array(
            proposals
                .iter()
                .map(|proposal| json!(proposal.note_id))
                .collect(),
        )
    } else {
        serde_json::to_value(&proposals)?
    };
    let text = proposals
        .iter()
        .map(|proposal| {
            if ids {
                proposal.note_id.to_string()
            } else {
                format!(
                    "{}\t{}\t{}\t{}",
                    proposal.note_id,
                    proposal.proposed_kind,
                    proposal.proposed_status,
                    proposal.confidence
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Output {
        command: "triage.list",
        data,
        page: None,
        text,
    })
}

async fn show(note_id: String, session: &Session) -> Result<Output> {
    let note_id = NoteId::new(note_id);
    find_note(session, &note_id).await?;
    let proposal = session.notes().await?.proposal_for_note(&note_id).await?;
    let text = proposal
        .as_ref()
        .map(format_proposal)
        .unwrap_or_else(|| format!("no triage proposal for {note_id}"));
    Ok(Output {
        command: "triage.show",
        data: json!({ "note_id": note_id, "proposal": proposal }),
        page: None,
        text,
    })
}

async fn commit(args: crate::args::TriageCommitArgs, session: &Session) -> Result<Output> {
    validate_tags(&args.tags)?;
    let note_id = NoteId::new(args.note_id);
    find_note(session, &note_id).await?;
    let policy = Session::triage_policy(&session.connection).await?;
    validate_kind(&policy.kinds, &args.kind)?;
    let existing = session.notes().await?.proposal_for_note(&note_id).await?;
    let mut proposal = existing.unwrap_or_else(|| TriageProposal {
        note_id: note_id.clone(),
        proposed_status: policy.triaged_status.clone(),
        ..Default::default()
    });
    proposal.note_id = note_id.clone();
    proposal.proposed_kind = args.kind;
    if let Some(status) = args.status {
        proposal.proposed_status = status;
    }
    for tag in args.tags {
        if !proposal.proposed_tags.contains(&tag) {
            proposal.proposed_tags.push(tag);
        }
    }
    let changed = session.notes().await?.commit_proposal(proposal).await?;
    let note = find_note(session, &note_id).await?;
    Ok(Output {
        command: "triage.commit",
        data: json!({ "changed": changed, "note": note }),
        page: None,
        text: if changed {
            "triage proposal committed".to_string()
        } else {
            "triage proposal unchanged".to_string()
        },
    })
}

fn validate_tags(tags: &[String]) -> Result<()> {
    if let Some(tag) = tags
        .iter()
        .find(|tag| !tag.contains('/') || tag.starts_with("kind/") || tag.starts_with("status/"))
    {
        return Err(CliError::invalid(format!(
            "tag must be strictly namespaced and not use kind/status: {tag}"
        ))
        .into());
    }
    Ok(())
}

fn validate_kind(kinds: &[String], kind: &str) -> Result<()> {
    if kinds.iter().any(|candidate| candidate == kind) {
        Ok(())
    } else {
        Err(CliError::invalid(format!("unknown note kind: {kind}")).into())
    }
}

fn format_proposal(proposal: &TriageProposal) -> String {
    format!(
        "{}\nkind: {}\nstatus: {}\ntags: {}\nconfidence: {}\nreason: {}",
        proposal.note_id,
        proposal.proposed_kind,
        proposal.proposed_status,
        proposal.proposed_tags.join(", "),
        proposal.confidence,
        proposal.reason
    )
}
