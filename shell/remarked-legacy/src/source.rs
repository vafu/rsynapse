use super::client::{NOTES_IFACE, NOTES_ROOT, NotesClient, SERVICE};
use futures_util::StreamExt;
use notebook_api::note::Note;
use proj_model::{GoalIdentityInfo, ProjectInfo};
use shell_core::source::{
    self, Observable,
    dbus::{self, Bus},
    rx::Observable as _,
};

#[derive(Clone, Debug, Default)]
pub(crate) enum Catalog {
    #[default]
    Loading,
    Unavailable,
    Ready {
        projects: Vec<ProjectInfo>,
        tasks: Vec<GoalIdentityInfo>,
        active: Vec<String>,
    },
}

#[derive(Clone, Debug, Default)]
pub(crate) enum Notes {
    #[default]
    Loading,
    NoProject,
    Error(String),
    Ready(String, Vec<Note>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Notebook {
    pub path: String,
    pub generation: u64,
}

pub(crate) fn catalog() -> Observable<Catalog> {
    source::switch_map(
        dbus::service_owner(Bus::Session, proj_model::BUS_NAME),
        |owner| {
            if owner.is_none() {
                return source::once(Catalog::Unavailable);
            }
            source::proj::projects()
                .combine_latest(source::proj::goal_identities(), |mut projects, tasks| {
                    projects.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                    (projects, tasks)
                })
                .combine_latest(
                    source::proj::goal_tracking(),
                    |(projects, tasks), tracking| Catalog::Ready {
                        projects,
                        tasks,
                        active: tracking
                            .into_iter()
                            .filter(|task| task.selected)
                            .map(|task| task.instance_id)
                            .collect(),
                    },
                )
                .start_with(vec![Catalog::Loading])
                .box_it()
        },
    )
}

pub(crate) fn notes(notebook: Option<Notebook>) -> Observable<Notes> {
    let Some(notebook) = notebook else {
        return source::once(Notes::NoProject);
    };
    source::switch_map(dbus::service_owner(Bus::Session, SERVICE), move |owner| {
        if owner.is_none() {
            let path = notebook.path.clone();
            return source::from_task(move |sender| {
                let path = path.clone();
                async move {
                    // Request ordinary D-Bus activation. The owner stream switches
                    // to the live subscription as soon as the daemon appears.
                    if let Err(error) = NotesClient::open(&path, Vec::new()).await {
                        let _ = sender.send(Ok(Notes::Error(error))).await;
                    }
                }
            })
            .start_with(vec![Notes::Loading])
            .box_it();
        }
        let path = notebook.path.clone();
        // The note API is method/signal based, not a property or ObjectManager
        // collection. This task bridges its typed snapshot and invalidation stream
        // into the standard Observable lifecycle. No filesystem watcher/polling.
        source::from_task(move |sender| {
            let path = path.clone();
            async move {
                let run = async {
                    let conn = zbus::Connection::session()
                        .await
                        .map_err(|e| e.to_string())?;
                    let root = zbus::Proxy::new(&conn, SERVICE, NOTES_ROOT, NOTES_IFACE)
                        .await
                        .map_err(|e| e.to_string())?;
                    let mut changes = root
                        .receive_signal("NotebookChanged")
                        .await
                        .map_err(|e| e.to_string())?;
                    let client = NotesClient::open(&path, Vec::new()).await?;
                    client.initialize().await?;
                    loop {
                        let value = match client.list().await {
                            Ok(notes) => Notes::Ready(path.clone(), notes),
                            Err(e) => Notes::Error(e),
                        };
                        if sender.send(Ok(value)).await.is_err() {
                            return Ok::<_, String>(());
                        }
                        loop {
                            let event = changes
                                .next()
                                .await
                                .ok_or("Remarked change stream closed")?;
                            let (changed_path,): (String,) =
                                event.body().deserialize().map_err(|e| e.to_string())?;
                            if changed_path == path {
                                break;
                            }
                        }
                    }
                }
                .await;
                if let Err(error) = run {
                    let _ = sender.send(Ok(Notes::Error(error))).await;
                }
            }
        })
        .start_with(vec![Notes::Loading])
        .box_it()
    })
}
