use clap::{Args, Subcommand, ValueEnum};

#[derive(Debug, Subcommand)]
pub enum ProjectTaskCommand {
    /// List project tasks.
    List(TaskListArgs),
    /// Create a task, or update it when the key already exists.
    Add(TaskCreateArgs),
    /// Create a named task idempotently.
    Set(TaskCreateArgs),
    /// Edit a task title or description.
    Update(TaskUpdateArgs),
    /// Show the full task record as JSON.
    Show(TaskRefArgs),
    /// Set a task status.
    Status(TaskStatusArgs),
    /// Mark a task completed.
    Complete(TaskRefArgs),
    /// Cancel a task without recording completion.
    Cancel(TaskRefArgs),
    /// Mark in progress and select as the project's active task.
    Start(TaskRefArgs),
    /// Read or clear the project's active task.
    Active(TaskActiveArgs),
    /// Manage Remarked notes attached to a task.
    Note {
        #[command(subcommand)]
        command: TaskNoteCommand,
    },
    /// Pin the implicit task for this project.
    Checkpoint(TaskCheckpointArgs),
    /// Clear the pinned implicit task for this project.
    Uncheckpoint,
    /// Remove a task while retaining history and notes.
    Remove(TaskRefArgs),
}

#[derive(Debug, Args)]
pub struct TaskListArgs {
    #[arg(long)]
    pub all: bool,
    #[arg(long)]
    pub date: Option<String>,
}

#[derive(Debug, Args)]
pub struct TaskCreateArgs {
    /// Optional stable ID; otherwise a UUID is generated.
    pub id: Option<String>,
    #[arg(long)]
    pub title: String,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long)]
    pub parent: Option<String>,
    #[arg(long)]
    pub active: bool,
    #[arg(long, default_value = "task", value_parser = ["task", "outcome", "habit"])]
    pub kind: String,
    #[arg(long, default_value = "")]
    pub success: String,
    #[arg(long, default_value = "medium", value_parser = ["low", "medium", "high"])]
    pub priority: String,
    #[arg(long)]
    pub date: Option<String>,
}

#[derive(Debug, Args)]
pub struct TaskUpdateArgs {
    pub id: String,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long)]
    pub date: Option<String>,
}

#[derive(Debug, Args)]
pub struct TaskRefArgs {
    pub id: String,
    #[arg(long)]
    pub date: Option<String>,
}

#[derive(Debug, Args)]
pub struct TaskStatusArgs {
    pub id: String,
    #[arg(value_enum)]
    pub status: TaskStatusValue,
    #[arg(long)]
    pub date: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TaskStatusValue {
    Planned,
    #[value(alias = "todo")]
    Todo,
    InProgress,
    Completed,
    Deferred,
    Cancelled,
}

impl TaskStatusValue {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Planned | Self::Todo => "planned",
            Self::InProgress => "in-progress",
            Self::Completed => "completed",
            Self::Deferred => "deferred",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Args)]
pub struct TaskActiveArgs {
    pub id: Option<String>,
    #[arg(long)]
    pub clear: bool,
}

#[derive(Debug, Subcommand)]
pub enum TaskNoteCommand {
    /// Create a task note and print its path. Main notes are idempotent.
    Add(TaskNoteAddArgs),
    /// List notes attached to a task.
    List(TaskNoteListArgs),
    /// Print a task note's path, revision, tags, and body.
    Show(TaskNoteShowArgs),
    /// Print only a task note's path.
    Path(TaskNotePathArgs),
    /// Validate a task note's frontmatter contract.
    #[command(alias = "verify")]
    Check(TaskNoteCheckArgs),
    /// Delete a task note.
    Delete(TaskNoteDeleteArgs),
}

#[derive(Debug, Args)]
pub struct TaskNoteAddArgs {
    #[arg(long, default_value = "main")]
    pub role: String,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long)]
    pub kind: Option<String>,
    #[arg(long = "tag")]
    pub tags: Vec<String>,
    /// Body text, `-` for stdin, or omit for a generated placeholder.
    #[arg(long, value_name = "TEXT")]
    pub body: Option<String>,
    /// Create a new note even when a main note already exists.
    #[arg(long)]
    pub new: bool,
}

#[derive(Debug, Args)]
pub struct TaskNoteListArgs {
    #[arg(long)]
    pub role: Option<String>,
}

#[derive(Debug, Args)]
pub struct TaskNoteShowArgs {
    pub note_id: String,
}

#[derive(Debug, Args)]
pub struct TaskNotePathArgs {
    #[arg(long)]
    pub role: Option<String>,
}

#[derive(Debug, Args)]
pub struct TaskNoteCheckArgs {
    /// Note file path, or a notebook-relative note id.
    pub target: String,
    /// Apply safe repairs: add the task tag and a role, drop inbox, add a kind.
    #[arg(long)]
    pub fix: bool,
}

#[derive(Debug, Args)]
pub struct TaskNoteDeleteArgs {
    pub note_id: String,
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct TaskCheckpointArgs {
    pub task: String,
}

#[derive(Debug, Subcommand)]
pub enum MarkdownTaskCommand {
    /// List checkbox tasks in the scoped task-list note.
    List(MarkdownTaskListArgs),
    /// Capture a root checkbox task.
    Capture(MarkdownTaskTextArgs),
    /// Append a task to an explicitly selected task-list revision.
    Append(MarkdownTaskAppendArgs),
    /// Insert a nested task beneath a task reference.
    Subtask(MarkdownTaskSubtaskArgs),
    /// Set one checkbox task status.
    SetStatus(MarkdownTaskSetStatusArgs),
    /// Complete checkbox tasks.
    Complete(MarkdownTaskRefsArgs),
    /// Reopen checkbox tasks.
    Reopen(MarkdownTaskRefsArgs),
}

#[derive(Debug, Args)]
pub struct MarkdownTaskListArgs {
    #[arg(long, value_enum, default_value_t = MarkdownTaskFilter::Open)]
    pub status: MarkdownTaskFilter,
    #[arg(long)]
    pub note: Option<String>,
    #[arg(long)]
    pub roots: bool,
    #[arg(long, default_value_t = 100)]
    pub limit: u64,
    #[arg(long)]
    pub ids: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum MarkdownTaskFilter {
    Open,
    Completed,
    All,
}

#[derive(Debug, Args)]
pub struct MarkdownTaskTextArgs {
    pub text: Option<String>,
}

#[derive(Debug, Args)]
pub struct MarkdownTaskAppendArgs {
    pub note_id: String,
    #[arg(long)]
    pub revision: String,
    pub text: Option<String>,
}

#[derive(Debug, Args)]
pub struct MarkdownTaskSubtaskArgs {
    pub task_ref: String,
    pub text: Option<String>,
}

#[derive(Debug, Args)]
pub struct MarkdownTaskSetStatusArgs {
    pub task_ref: String,
    #[arg(value_enum)]
    pub status: MarkdownTaskStatusValue,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum MarkdownTaskStatusValue {
    Open,
    Completed,
}

#[derive(Debug, Args)]
pub struct MarkdownTaskRefsArgs {
    #[arg(required = true)]
    pub task_refs: Vec<String>,
}
