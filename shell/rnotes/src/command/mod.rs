mod common;
mod markdown_task;
mod note;
mod search;
mod status;
mod task;
mod task_note;
mod triage;

use anyhow::Result;

use crate::Session;
use crate::args::Command;
use crate::output::Output;
use crate::scope::Resolved;

pub async fn execute(command: Command, session: &Session, resolved: &Resolved) -> Result<Output> {
    match command {
        Command::Status => status::status(session, resolved).await,
        Command::Capabilities => status::capabilities(session).await,
        Command::Note { command } => note::execute(command, session, resolved).await,
        Command::Task { command } => task::execute(command, session, resolved).await,
        Command::MarkdownTask { command } => {
            markdown_task::execute(command, session, resolved).await
        }
        Command::Search(args) => search::execute(args, session).await,
        Command::Tags => note::tags(session).await,
        Command::Triage { command } => triage::execute(command, session).await,
    }
}
