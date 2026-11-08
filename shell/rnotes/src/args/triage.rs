use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub enum TriageCommand {
    Policy,
    Process(TriageNoteArgs),
    ProcessInbox,
    List(TriageListArgs),
    Show(TriageNoteArgs),
    Commit(TriageCommitArgs),
}

#[derive(Debug, Args)]
pub struct TriageNoteArgs {
    pub note_id: String,
}

#[derive(Debug, Args)]
pub struct TriageListArgs {
    #[arg(long)]
    pub ids: bool,
}

#[derive(Debug, Args)]
pub struct TriageCommitArgs {
    pub note_id: String,
    #[arg(long)]
    pub kind: String,
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long = "tag")]
    pub tags: Vec<String>,
}
