use super::super::selected_project::selected_project_status;
use data_sources::{self, Observable, proj::ActiveTask, rx::Observable as _};

#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::widgets::bar) struct GoalBarVm {
    pub project_id: Option<String>,
    pub title: String,
    pub description: String,
    pub tooltip: String,
    pub active: String,
}

pub(in crate::widgets::bar) fn goal_bar(output: Option<String>) -> Observable<GoalBarVm> {
    let key = format!("{output:?}");
    data_sources::shared_by_key("rsynapse.goal-bar", key, move || {
        goal_bar_inner(output.clone())
    })
}

fn goal_bar_inner(output: Option<String>) -> Observable<GoalBarVm> {
    let project = selected_project_status(output)
        .map(|view| view.project.and_then(|project| project.project_id))
        .distinct_until_changed()
        .box_it();
    data_sources::switch_map(project, |id| {
        let Some(id) = id else {
            return data_sources::once(GoalBarVm::default());
        };
        data_sources::proj::active_task(id.clone())
            .map(move |task| view(id.clone(), task))
            .distinct_until_changed()
            .box_it()
    })
    .distinct_until_changed()
    .box_it()
}

fn view(project: String, task: Option<ActiveTask>) -> GoalBarVm {
    let Some(task) = task else {
        return GoalBarVm {
            project_id: Some(project),
            title: "Choose task".into(),
            description: "No active task".into(),
            ..Default::default()
        };
    };
    let tooltip = format!("{}\n\n{}", task.title, task.status);
    GoalBarVm {
        project_id: Some(project),
        title: task.title,
        description: task.status,
        tooltip,
        active: task.id,
    }
}
