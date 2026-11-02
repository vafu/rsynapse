//! Reactive data sources for the remarked overlay.
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
