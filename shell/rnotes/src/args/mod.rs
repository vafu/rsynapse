mod note;
mod task;
mod triage;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub use note::*;
pub use task::*;
pub use triage::*;

use crate::output::Format;

#[derive(Debug, Parser)]
#[command(
    name = "rnotes",
    version,
    about = "Project tasks and Remarked notes for the Rsynapse shell"
)]
pub struct Cli {
    /// Project directory. Defaults to the current directory.
    #[arg(long, global = true, value_name = "PATH")]
    pub project: Option<PathBuf>,

    /// projd task key, ID, or immutable instance UUID.
    #[arg(long, global = true, value_name = "TASK")]
    pub task: Option<String>,

    /// Explicit notebook directory. Skips project notebook resolution.
    #[arg(long, global = true, value_name = "PATH")]
    pub notebook: Option<PathBuf>,

    /// Skip project resolution for global tasks or the daemon default notebook.
    #[arg(long, global = true)]
    pub global: bool,

    /// Additional notebook scope tag. Repeat for multiple tags.
    #[arg(long = "scope-tag", global = true)]
    pub scope_tag: Vec<String>,

    /// Output protocol.
    #[arg(long, global = true, value_enum, default_value_t = Format::Text)]
    pub format: Format,

    /// Suppress successful text output.
    #[arg(long, global = true)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Show notebook status and effective scope.
    Status,
    /// List enabled service capabilities.
    Capabilities,
    /// Manage Remarked notes.
    Note {
        #[command(subcommand)]
        command: NoteCommand,
    },
    /// Manage projd project tasks.
    Task {
        #[command(subcommand)]
        command: ProjectTaskCommand,
    },
    /// Manage Markdown checkbox tasks inside the notebook.
    #[command(name = "markdown-task", alias = "md-task")]
    MarkdownTask {
        #[command(subcommand)]
        command: MarkdownTaskCommand,
    },
    /// Lexically search scoped notes.
    Search(SearchArgs),
    /// List notebook tags.
    Tags,
    /// Process, inspect, and commit triage proposals.
    Triage {
        #[command(subcommand)]
        command: TriageCommand,
    },
}
