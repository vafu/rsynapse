//! Reactive data sources for the remarked overlay.
use futures_util::StreamExt;
use notebook_api::note::Note;
use proj_model::{GoalIdentityInfo, ProjectInfo};
use shell_core::source::{
    self, Observable,
    dbus::{self, Bus},
    rx::Observable as _,
};

use crate::client::{NotesClient, NOTES_IFACE, NOTES_ROOT, SERVICE};

#[derive(Clone, Debug, Default)]
pub(crate) enum Catalog {
    #[default]
    Loading,
    Unavailable,
    NoProject,
    Ready {
        project: Option<ProjectInfo>,
        tasks: Vec<GoalIdentityInfo>,
        active: Vec<String>,
    },
}

impl Catalog {
    pub(crate) fn tasks(&self) -> &[GoalIdentityInfo] {
        match self {
            Catalog::Ready { tasks, .. } => tasks,
            _ => &[],
        }
    }

    pub(crate) fn project(&self) -> Option<&ProjectInfo> {
        match self {
            Catalog::Ready { project, .. } => project.as_ref(),
            _ => None,
        }
    }
}

/// Returns a live catalog scoped to the provided project ID.
pub(crate) fn catalog(project_id: Observable<Option<String>>) -> Observable<Catalog> {
    source::switch_map(project_id, |project_id| {
        let Some(project_id) = project_id else {
            return source::once(Catalog::NoProject);
        };
        source::switch_map(
            dbus::service_owner(Bus::Session, proj_model::BUS_NAME),
            move |owner| {
                if owner.is_none() {
                    return source::once(Catalog::Unavailable);
                }
                source::proj::projects()
                    .combine_latest(source::proj::goal_identities(), {
                        let project_id = project_id.clone();
                        move |projects, tasks| {
                            let project = projects.into_iter().find(|p| p.id == project_id);
                            let tasks = tasks
                                .into_iter()
                                .filter(|t| t.goal.project_id == project_id)
                                .collect::<Vec<_>>();
                            (project, tasks)
                        }
                    })
                    .combine_latest(
                        source::proj::goal_tracking(),
                        |(project, tasks), tracking| Catalog::Ready {
                            project,
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
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Notebook {
    pub path: String,
    pub generation: u64,
}

#[derive(Clone, Debug, Default)]
pub(crate) enum Notes {
    #[default]
    Loading,
    NoNotebook,
    Error(String),
    Ready(String, Vec<Note>),
}

pub(crate) fn task_tag(instance: &str) -> String {
    format!("task/{instance}")
}

/// Returns a live notes snapshot scoped to a notebook and optional task tag.
pub(crate) fn notes(
    notebook: Observable<Option<Notebook>>,
    task: Observable<Option<String>>,
) -> Observable<Notes> {
    source::switch_map(
        notebook
            .combine_latest(task, |notebook, task| (notebook, task))
            .box_it(),
        |(notebook, task): (Option<Notebook>, Option<String>)| {
            let Some(notebook) = notebook else {
                return source::once(Notes::NoNotebook);
            };
            let tags = task.map(|t| vec![task_tag(&t)]).unwrap_or_default();
            source::switch_map(dbus::service_owner(Bus::Session, SERVICE), move |owner| {
                if owner.is_none() {
                    let path = notebook.path.clone();
                    let tags = tags.clone();
                    return source::from_task(move |sender| {
                        let path = path.clone();
                        let tags = tags.clone();
                        async move {
                            if let Err(error) = NotesClient::open(&path, tags).await {
                                let _ = sender.send(Ok(Notes::Error(error))).await;
                            }
                        }
                    })
                    .start_with(vec![Notes::Loading])
                    .box_it();
                }
                let path = notebook.path.clone();
                let tags = tags.clone();
                source::from_task(move |sender| {
                    let path = path.clone();
                    let tags = tags.clone();
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
                            let client = NotesClient::open(&path, tags).await?;
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
        },
    )
}
