//! Typed D-Bus client for the remarked note service.
use notebook_api::{
    note::{CommandResult, Note, NoteBody, NoteId, NoteRevision, TagCount},
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
        // `zk init` fails when a notebook already exists, so bootstrap is
        // best-effort: an existing notebook is already usable.
        let result: CommandResult = match self.proxy.call("BootstrapNotebook", &()).await {
            Ok(result) => result,
            Err(_) => return Ok(()),
        };
        let _ = result;
        Ok(())
    }

    pub(crate) async fn list(&self) -> Result<Vec<Note>, String> {
        self.proxy.call("ListScopeNotes", &()).await.map_err(error)
    }

    /// The service exposes tag names, not counts. Derive counts from the scoped
    /// notes so the overlay can show "N notes" per task without a nonexistent
    /// `ListTagCounts` method.
    pub(crate) async fn tag_counts(&self) -> Result<Vec<TagCount>, String> {
        let notes = self.list().await?;
        let mut counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        for note in notes {
            for tag in note.tags {
                *counts.entry(tag).or_default() += 1;
            }
        }
        Ok(counts
            .into_iter()
            .map(|(name, count)| TagCount { name, count })
            .collect())
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
    ) -> Result<notebook_api::note::NoteMutationResult, String> {
        self.proxy
            .call("ReplaceNoteBody", &(id, revision, text))
            .await
            .map_err(error)
    }
}

#[derive(Clone, Debug)]
pub(crate) enum TaskAction {
    Status {
        key: String,
        status: String,
    },
    Start {
        key: String,
        project: String,
    },
    Clear {
        project: String,
    },
    Delete {
        key: String,
    },
    Create {
        project: proj_model::ProjectInfo,
        parent: String,
        title: String,
        description: String,
    },
    Edit {
        key: String,
        title: String,
        description: String,
    },
}

pub(crate) async fn task_action(action: TaskAction) -> Result<(), String> {
    use proj_model::{GoalInfo, api::ManagerProxy};
    let conn = zbus::Connection::session().await.map_err(error)?;
    let proxy = ManagerProxy::new(&conn).await.map_err(error)?;
    match action {
        TaskAction::Status { key, status } => {
            let _: GoalInfo = proxy.set_goal_status(&key, &status).await.map_err(error)?;
        }
        TaskAction::Start { key, project } => {
            let _: GoalInfo = proxy
                .set_goal_status(&key, "in-progress")
                .await
                .map_err(error)?;
            let _: () = proxy.set_active_goal(&project, &key).await.map_err(error)?;
        }
        TaskAction::Clear { project } => {
            let _: () = proxy.set_active_goal(&project, "").await.map_err(error)?;
        }
        TaskAction::Delete { key } => {
            let (date, id) = key.split_once('/').unwrap_or(("", &key));
            let _: bool = proxy.remove_goal(date, id).await.map_err(error)?;
        }
        TaskAction::Create {
            project,
            parent,
            title,
            description,
        } => {
            let goal = GoalInfo {
                id: uuid::Uuid::new_v4().simple().to_string(),
                title,
                description,
                parent_id: parent,
                project: project.cwd,
                project_id: project.id,
                status: "planned".into(),
                kind: "task".into(),
                priority: "medium".into(),
                ..Default::default()
            };
            let _: GoalInfo = proxy.create_goal(&goal).await.map_err(error)?;
        }
        TaskAction::Edit {
            key,
            title,
            description,
        } => {
            let fields = std::collections::HashMap::from([
                ("title".to_owned(), title),
                ("description".to_owned(), description),
            ]);
            let _: GoalInfo = proxy.edit_goal(&key, fields).await.map_err(error)?;
        }
    }
    Ok(())
}

fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
