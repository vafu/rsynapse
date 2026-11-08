use clap::{Args, Subcommand, ValueEnum};

#[derive(Debug, Subcommand)]
pub enum NoteCommand {
    /// Create a fleeting note in the resolved scope.
    Capture(NoteCaptureArgs),
    /// List notes in the resolved scope.
    List(NoteListArgs),
    /// Read full notes.
    Get(NoteGetArgs),
    /// Show a note with its links, backlinks, tasks, and triage proposal.
    Context(NoteContextArgs),
    /// Replace one note body against an exact revision.
    Replace(NoteReplaceArgs),
    /// Delete one note against an exact revision.
    Delete(NoteDeleteArgs),
    /// List outgoing and incoming links.
    Links(NoteLinksArgs),
}

#[derive(Debug, Args)]
pub struct NoteCaptureArgs {
    /// Body text, `-` for stdin, or omit to read stdin.
    pub body: Option<String>,
    #[arg(long = "tag")]
    pub tags: Vec<String>,
}

#[derive(Debug, Args)]
pub struct NoteListArgs {
    #[arg(long)]
    pub inbox: bool,
    #[arg(long)]
    pub kind: Option<String>,
    #[arg(long = "tag")]
    pub tags: Vec<String>,
    #[arg(long, default_value_t = 50)]
    pub limit: u64,
    #[arg(long)]
    pub ids: bool,
}

#[derive(Debug, Args)]
pub struct NoteGetArgs {
    #[arg(required = true)]
    pub note_ids: Vec<String>,
    #[arg(long)]
    pub body: bool,
}

#[derive(Debug, Args)]
pub struct NoteContextArgs {
    pub note_id: String,
}

#[derive(Debug, Args)]
pub struct NoteReplaceArgs {
    pub note_id: String,
    #[arg(long)]
    pub revision: String,
    /// Body text, `-` for stdin, or omit to read stdin.
    pub body: Option<String>,
}

#[derive(Debug, Args)]
pub struct NoteDeleteArgs {
    pub note_id: String,
    #[arg(long)]
    pub revision: String,
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct NoteLinksArgs {
    pub note_id: String,
    #[arg(long, value_enum, default_value_t = LinkDirection::Both)]
    pub direction: LinkDirection,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum LinkDirection {
    Out,
    In,
    Both,
}

#[derive(Debug, Args)]
pub struct SearchArgs {
    #[arg(long, value_enum, default_value_t = SearchMode::Lexical)]
    pub mode: SearchMode,
    #[arg(long = "tag")]
    pub tags: Vec<String>,
    #[arg(long, default_value_t = 8)]
    pub limit: u64,
    #[arg(long, default_value_t = 240)]
    pub snippet_chars: u64,
    #[arg(required = true)]
    pub query: Vec<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SearchMode {
    Lexical,
}
