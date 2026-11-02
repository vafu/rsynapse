//! Typed D-Bus client for the remarked note service.
use notebook_api::{
    note::{CommandResult, Note, NoteBody, NoteId, NoteRevision, PatchResult},
    notebook_scope::{NotebookHandle, NotebookScope},
};

pub(crate) const SERVICE: &str = "org.remarked.Service";
pub(crate) const NOTES_ROOT: &str = "/org/remarked/Notes";
pub(crate) const NOTES_IFACE: &str = "org.remarked.Notes1";

pub(crate) struct NotesClient {
    proxy: zbus::Proxy<'static>,
}

impl NotesClient {
    pub(crate) async fn open(path: &str, tags: Vec<String>) -> Result<Self, String> {
        let conn = zbus::Connection::session().await.map_err(error)?;
        let manager = zbus::Proxy::new(
            &conn,
            SERVICE,
            "/org/remarked/Notebooks",
            "org.remarked.Notebooks1",
        )
        .await
        .map_err(error)?;
        let handle: NotebookHandle = manager
            .call(
                "OpenNotebook",
                &(NotebookScope {
                    path: path.into(),
                    tags,
                },),
            )
            .await
            .map_err(error)?;
        if handle.object_path.is_empty() {
            return Err("Remarked could not open this notebook".into());
        }
        let proxy = zbus::Proxy::new_owned(
            conn.clone(),
            SERVICE,
            format!("{}/Notes", handle.object_path),
            NOTES_IFACE,
        )
        .await
        .map_err(error)?;
        Ok(Self { proxy })
    }

    pub(crate) async fn initialize(&self) -> Result<(), String> {
        let result: CommandResult = self
            .proxy
            .call("BootstrapNotebook", &())
            .await
            .map_err(error)?;
        if !result.changed && result.message != "notebook initialized" {
            return Err(result.message);
        }
        Ok(())
    }

    pub(crate) async fn list(&self) -> Result<Vec<Note>, String> {
        self.proxy.call("ListScopeNotes", &()).await.map_err(error)
    }

    pub(crate) async fn body(&self, id: &NoteId) -> Result<NoteBody, String> {
        self.proxy.call("NoteBody", &(id,)).await.map_err(error)
    }

    pub(crate) async fn capture(&self, text: &str) -> Result<NoteId, String> {
        self.initialize().await?;
        self.proxy.call("CreateNote", &(text,)).await.map_err(error)
    }

    pub(crate) async fn save(
        &self,
        id: &NoteId,
        revision: &NoteRevision,
        text: &str,
        tags: Vec<String>,
    ) -> Result<PatchResult, String> {
        self.proxy
            .call("SaveNote", &(id, revision, text, tags))
            .await
            .map_err(error)
    }
}

fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
