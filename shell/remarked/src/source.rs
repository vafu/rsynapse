//! Reactive data sources for the remarked overlay.
use futures_util::StreamExt;
use notebook_api::note::Note;
use proj_model::{GoalIdentityInfo, ProjectInfo};
use shell_core::source::{
    self, Observable,
    dbus::{self, Bus},
    rx::Observable as _,
};

use crate::client::{NOTES_IFACE, NOTES_ROOT, NotesClient, SERVICE};
use crate::model;

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

    pub(crate) fn active(&self) -> &[String] {
        match self {
            Catalog::Ready { active, .. } => active,
            _ => &[],
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
    Ready {
        path: String,
        task: Option<String>,
        notes: Vec<Note>,
    },
}

impl Notes {
    pub(crate) fn ready_notes(&self) -> Option<&[Note]> {
        match self {
            Self::Ready { notes, .. } => Some(notes),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum TagCounts {
    #[default]
    Loading,
    Error(String),
    Ready(std::collections::HashMap<String, u32>),
}

impl TagCounts {
    pub(crate) fn count(&self, tag: &str) -> Option<u32> {
        match self {
            TagCounts::Ready(counts) => Some(*counts.get(tag).unwrap_or(&0)),
            _ => None,
        }
    }
}

/// Returns note counts per tag for the notebook. One `zk tag list` call; the
/// scope's tag filter is irrelevant because tag counts are notebook-wide.
pub(crate) fn tag_counts(notebook: Observable<Option<Notebook>>) -> Observable<TagCounts> {
    source::switch_map(notebook, |notebook| {
        let Some(notebook) = notebook else {
            return source::once(TagCounts::Loading);
        };
        source::switch_map(dbus::service_owner(Bus::Session, SERVICE), move |owner| {
            let path = notebook.path.clone();
            if owner.is_none() {
                return source::once(TagCounts::Error("Remarked is unavailable".into()));
            }
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
                        loop {
                            let value = match client.tag_counts().await {
                                Ok(counts) => TagCounts::Ready(
                                    counts
                                        .into_iter()
                                        .map(|tag| (tag.name, tag.count))
                                        .collect(),
                                ),
                                Err(error) => TagCounts::Error(error),
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
                        let _ = sender.send(Ok(TagCounts::Error(error))).await;
                    }
                }
            })
            .start_with(vec![TagCounts::Loading])
            .box_it()
        })
    })
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
            let tags = task
                .as_ref()
                .map(|t| vec![model::task_tag(t)])
                .unwrap_or_default();
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
                let task = task.clone();
                source::from_task(move |sender| {
                    let path = path.clone();
                    let tags = tags.clone();
                    let task = task.clone();
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
                                    Ok(notes) => Notes::Ready {
                                        path: path.clone(),
                                        task: task.clone(),
                                        notes,
                                    },
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
