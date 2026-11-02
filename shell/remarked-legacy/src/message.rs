use crate::{client::TaskAction, model::Navigate, sources};
use notebook_api::note::{NoteBody, NoteId, PatchResult};

#[derive(Debug)]
pub enum WorkspaceInput {
    Source(sources::Msg),
    Open(Option<String>),
    Hide,
    Toggle(Option<String>),
    Navigate(Navigate),
    ProjectIndex(u32),
    Filter(String),
    Changed,
    Save,
    Reload,
    Retry,
    Render,
    Start,
    StopTracking,
    Status(u32),
    TaskDialog {
        subtask: bool,
        edit: bool,
    },
    TaskAction(TaskAction),
    TaskDone(Result<(), String>),
    Import,
    Loaded {
        path: String,
        id: NoteId,
        version: u64,
        result: Result<NoteBody, String>,
    },
    Saved {
        path: String,
        id: NoteId,
        text: String,
        tags: Vec<String>,
        result: Result<PatchResult, String>,
    },
    Created {
        path: String,
        result: Result<NoteBody, String>,
    },
    Prompt {
        response: String,
        target: Navigate,
    },
}

impl From<sources::Msg> for WorkspaceInput {
    fn from(value: sources::Msg) -> Self {
        Self::Source(value)
    }
}
