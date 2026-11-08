//! Native D-Bus access to the Remarked notebook service.
//!
//! The shell vendored the wire contract in `notebook-api`; this module is the
//! client side. It deliberately does not depend on `remarked-dbus` so the shell
//! builds from a clean checkout without the Remarked workspace.
use anyhow::{Context, Result};
use notebook_api::{
    note::{
        Note, NoteBody, NoteCapture, NoteId, NoteMutationResult, NotePage, NoteQuery,
        NoteReference, NoteRevision,
    },
    notebook_scope::{NotebookHandle, NotebookScope},
    search::{LexicalSearchQuery, SearchPage},
    task::{TaskItem, TaskLocator, TaskStatus},
    triage::{TriagePolicy, TriageProposal},
    workspace::WorkspaceStatus,
};
use zbus::proxy;

#[proxy(
    interface = "org.remarked.Workspace1",
    default_service = "org.remarked.Service",
    default_path = "/org/remarked/Workspace"
)]
trait Workspace {
    fn status(&self) -> zbus::Result<WorkspaceStatus>;
    fn notebook_scope(&self) -> zbus::Result<NotebookScope>;
    fn triage_policy(&self) -> zbus::Result<TriagePolicy>;
}

#[proxy(
    interface = "org.remarked.Notebooks1",
    default_service = "org.remarked.Service",
    default_path = "/org/remarked/Notebooks"
)]
trait Notebooks {
    fn open_notebook(&self, scope: NotebookScope) -> zbus::Result<NotebookHandle>;
}

#[proxy(
    interface = "org.remarked.Notes1",
    default_service = "org.remarked.Service"
)]
trait Notes {
    fn list_scope_notes(&self) -> zbus::Result<Vec<Note>>;
    fn query_scope_notes(&self, query: NoteQuery) -> zbus::Result<NotePage>;
    fn list_tags(&self) -> zbus::Result<Vec<String>>;
    fn linked_notes(&self, note_id: NoteId) -> zbus::Result<Vec<Note>>;
    fn backlinked_notes(&self, note_id: NoteId) -> zbus::Result<Vec<Note>>;
    fn resolve_note_reference(&self, reference: NoteReference) -> zbus::Result<Vec<Note>>;
    fn note_body(&self, note_id: NoteId) -> zbus::Result<NoteBody>;
    fn capture_note(&self, capture: NoteCapture) -> zbus::Result<NoteId>;
    fn replace_note_body(
        &self,
        note_id: NoteId,
        base_revision: NoteRevision,
        body: &str,
    ) -> zbus::Result<NoteMutationResult>;
    fn bootstrap_notebook(&self) -> zbus::Result<notebook_api::note::CommandResult>;
    fn delete_note(
        &self,
        note_id: NoteId,
        base_revision: NoteRevision,
    ) -> zbus::Result<NoteMutationResult>;
}

#[proxy(
    interface = "org.remarked.Tasks1",
    default_service = "org.remarked.Service"
)]
trait Tasks {
    fn capture_task(&self, text: &str) -> zbus::Result<TaskItem>;
    fn append_task(
        &self,
        note_id: NoteId,
        note_revision: NoteRevision,
        text: &str,
    ) -> zbus::Result<TaskItem>;
    fn create_subtask(&self, parent: TaskLocator, text: &str) -> zbus::Result<TaskItem>;
    fn list_scope_tasks(&self) -> zbus::Result<Vec<TaskItem>>;
    fn list_note_tasks(&self, note_id: NoteId) -> zbus::Result<Vec<TaskItem>>;
    fn resolve_task(&self, locator: TaskLocator) -> zbus::Result<TaskItem>;
    fn set_task_status(&self, task: TaskLocator, status: TaskStatus) -> zbus::Result<TaskItem>;
}

#[proxy(
    interface = "org.remarked.Search1",
    default_service = "org.remarked.Service"
)]
trait Search {
    fn lexical_search(&self, query: LexicalSearchQuery) -> zbus::Result<SearchPage>;
}

#[proxy(
    interface = "org.remarked.Capabilities1",
    default_service = "org.remarked.Service"
)]
trait Capabilities {
    fn capabilities(&self) -> zbus::Result<notebook_api::Capabilities>;
}

#[proxy(
    interface = "org.remarked.Triage1",
    default_service = "org.remarked.Service"
)]
trait Triage {
    fn list_proposals(&self) -> zbus::Result<Vec<TriageProposal>>;
    fn proposal_for_note(&self, note_id: NoteId) -> zbus::Result<Vec<TriageProposal>>;
    fn process_note(&self, note_id: NoteId) -> zbus::Result<bool>;
    fn process_inbox(&self) -> zbus::Result<u64>;
    fn commit_proposal(&self, proposal: TriageProposal) -> zbus::Result<bool>;
}

/// Client bound to one scoped notebook object tree.
pub struct Notebook {
    notes: NotesProxy<'static>,
    tasks: TasksProxy<'static>,
    search: SearchProxy<'static>,
    triage: TriageProxy<'static>,
    capabilities: CapabilitiesProxy<'static>,
}

impl Notebook {
    /// Opens a scoped notebook through the Remarked notebook manager.
    pub async fn open(connection: &zbus::Connection, scope: NotebookScope) -> Result<Self> {
        let manager = NotebooksProxy::builder(connection)
            .build()
            .await
            .context("connect to remarked notebook manager")?;
        let handle = manager
            .open_notebook(scope)
            .await
            .context("open notebook through remarked daemon")?;
        if handle.object_path.is_empty() {
            anyhow::bail!("remarked returned an empty notebook object path");
        }
        let base = handle.object_path.trim_end_matches('/');
        Self::at(connection, base).await
    }

    async fn at(connection: &zbus::Connection, base: &str) -> Result<Self> {
        Ok(Self {
            notes: NotesProxy::builder(connection)
                .path(format!("{base}/Notes"))?
                .build()
                .await
                .context("connect to remarked notes")?,
            tasks: TasksProxy::builder(connection)
                .path(format!("{base}/Tasks"))?
                .build()
                .await
                .context("connect to remarked tasks")?,
            search: SearchProxy::builder(connection)
                .path(format!("{base}/Search"))?
                .build()
                .await
                .context("connect to remarked search")?,
            triage: TriageProxy::builder(connection)
                .path(format!("{base}/Triage"))?
                .build()
                .await
                .context("connect to remarked triage")?,
            capabilities: CapabilitiesProxy::builder(connection)
                .path(format!("{base}/Capabilities"))?
                .build()
                .await
                .context("connect to remarked capabilities")?,
        })
    }

    pub async fn status(connection: &zbus::Connection) -> Result<WorkspaceStatus> {
        Ok(WorkspaceProxy::new(connection).await?.status().await?)
    }

    pub async fn scope(connection: &zbus::Connection) -> Result<NotebookScope> {
        Ok(WorkspaceProxy::new(connection)
            .await?
            .notebook_scope()
            .await?)
    }

    pub async fn triage_policy(connection: &zbus::Connection) -> Result<TriagePolicy> {
        Ok(WorkspaceProxy::new(connection)
            .await?
            .triage_policy()
            .await?)
    }

    pub async fn list_notes(&self) -> Result<Vec<Note>> {
        Ok(self.notes.list_scope_notes().await?)
    }

    pub async fn query_notes(&self, query: NoteQuery) -> Result<NotePage> {
        Ok(self.notes.query_scope_notes(query).await?)
    }

    pub async fn list_tags(&self) -> Result<Vec<String>> {
        Ok(self.notes.list_tags().await?)
    }

    pub async fn linked_notes(&self, id: &NoteId) -> Result<Vec<Note>> {
        Ok(self.notes.linked_notes(id.clone()).await?)
    }

    pub async fn backlinked_notes(&self, id: &NoteId) -> Result<Vec<Note>> {
        Ok(self.notes.backlinked_notes(id.clone()).await?)
    }

    pub async fn note_body(&self, id: &NoteId) -> Result<NoteBody> {
        Ok(self.notes.note_body(id.clone()).await?)
    }

    pub async fn capture_note(&self, capture: NoteCapture) -> Result<NoteId> {
        Ok(self.notes.capture_note(capture).await?)
    }

    /// Ensures a notebook exists. Safe to call repeatedly; the service returns
    /// an unchanged result for already-initialized notebooks.
    pub async fn bootstrap(&self) -> Result<()> {
        self.notes.bootstrap_notebook().await?;
        Ok(())
    }

    pub async fn replace_note(
        &self,
        id: &NoteId,
        revision: &NoteRevision,
        body: &str,
    ) -> Result<NoteMutationResult> {
        Ok(self
            .notes
            .replace_note_body(id.clone(), revision.clone(), body)
            .await?)
    }

    pub async fn delete_note(
        &self,
        id: &NoteId,
        revision: &NoteRevision,
    ) -> Result<NoteMutationResult> {
        Ok(self.notes.delete_note(id.clone(), revision.clone()).await?)
    }

    pub async fn capture_task(&self, text: &str) -> Result<TaskItem> {
        Ok(self.tasks.capture_task(text).await?)
    }

    pub async fn append_task(
        &self,
        note_id: &NoteId,
        revision: &NoteRevision,
        text: &str,
    ) -> Result<TaskItem> {
        Ok(self
            .tasks
            .append_task(note_id.clone(), revision.clone(), text)
            .await?)
    }

    pub async fn create_subtask(&self, parent: &TaskLocator, text: &str) -> Result<TaskItem> {
        Ok(self.tasks.create_subtask(parent.clone(), text).await?)
    }

    pub async fn list_scope_tasks(&self) -> Result<Vec<TaskItem>> {
        Ok(self.tasks.list_scope_tasks().await?)
    }

    pub async fn list_note_tasks(&self, note_id: &NoteId) -> Result<Vec<TaskItem>> {
        Ok(self.tasks.list_note_tasks(note_id.clone()).await?)
    }

    pub async fn resolve_task(&self, locator: &TaskLocator) -> Result<TaskItem> {
        Ok(self.tasks.resolve_task(locator.clone()).await?)
    }

    pub async fn set_task_status(
        &self,
        task: &TaskLocator,
        status: TaskStatus,
    ) -> Result<TaskItem> {
        Ok(self.tasks.set_task_status(task.clone(), status).await?)
    }

    pub async fn search(&self, query: LexicalSearchQuery) -> Result<SearchPage> {
        Ok(self.search.lexical_search(query).await?)
    }

    pub async fn list_proposals(&self) -> Result<Vec<TriageProposal>> {
        Ok(self.triage.list_proposals().await?)
    }

    pub async fn proposal_for_note(&self, note_id: &NoteId) -> Result<Option<TriageProposal>> {
        Ok(self
            .triage
            .proposal_for_note(note_id.clone())
            .await?
            .into_iter()
            .next())
    }

    pub async fn process_note(&self, note_id: &NoteId) -> Result<bool> {
        Ok(self.triage.process_note(note_id.clone()).await?)
    }

    pub async fn process_inbox(&self) -> Result<u64> {
        Ok(self.triage.process_inbox().await?)
    }

    pub async fn commit_proposal(&self, proposal: TriageProposal) -> Result<bool> {
        Ok(self.triage.commit_proposal(proposal).await?)
    }

    pub async fn capabilities(&self) -> Result<notebook_api::Capabilities> {
        Ok(self.capabilities.capabilities().await?)
    }
}
