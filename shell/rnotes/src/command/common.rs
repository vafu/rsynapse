use std::io::{self, Read};

use anyhow::{Context, Result};
use notebook_api::note::{Note, NoteId, NoteQuery};

use crate::Session;
use crate::output::CliError;

pub fn read_primary(value: Option<String>, name: &str) -> Result<String> {
    read_primary_preserving_newline(value, name, true)
}

/// Reads a primary argument or stdin. `trim_newlines` keeps single-line command
/// arguments tidy while leaving note bodies byte-exact.
pub fn read_primary_preserving_newline(
    value: Option<String>,
    name: &str,
    trim_newlines: bool,
) -> Result<String> {
    let mut value = match value {
        Some(value) if value != "-" => value,
        _ => {
            let mut input = String::new();
            io::stdin()
                .read_to_string(&mut input)
                .with_context(|| format!("read {name} from stdin"))?;
            input
        }
    };
    if trim_newlines {
        while value.ends_with('\n') || value.ends_with('\r') {
            value.pop();
        }
    }
    if value.trim().is_empty() {
        return Err(CliError::invalid(format!("{name} is required")).into());
    }
    Ok(value)
}

pub async fn find_note(session: &Session, note_id: &NoteId) -> Result<Note> {
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

pub fn validate_limit(limit: u64) -> Result<u64> {
    if limit == 0 {
        return Err(CliError::invalid("limit must be greater than zero").into());
    }
    Ok(limit)
}
