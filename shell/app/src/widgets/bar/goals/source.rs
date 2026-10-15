use super::super::selected_project::selected_project_status;
use shell_core::source::{
    self, Observable,
    proj::{GoalIdentityInfo, GoalInfo},
    rx::Observable as _,
};
mod catalog;

#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::widgets::bar) struct GoalBarVm {
    pub project_id: Option<String>,
    pub title: String,
    pub description: String,
    pub tooltip: String,
    pub active: String,
    context_keys: Vec<String>,
    pub(super) rows: Vec<GoalRow>,
}
impl GoalBarVm {
    pub fn has_goals(&self) -> bool {
        !self.rows.is_empty()
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct GoalRow {
    pub goal: GoalInfo,
    pub depth: usize,
    pub completed: bool,
    pub child_count: usize,
    pub completed_children: usize,
    pub instance_id: String,
    pub created_at_ms: u64,
}

pub(in crate::widgets::bar) fn goal_bar(output: Option<String>) -> Observable<GoalBarVm> {
    let project = selected_project_status(output)
        .map(|v| v.project.and_then(|p| p.project_id))
        .distinct_until_changed()
        .box_it();
    let checklist = source::switch_map(project, |id| {
        let Some(id) = id else {
            return source::once(GoalBarVm::default());
        };
        source::proj::goal_identities()
            .combine_latest(
                source::proj::active_goal(id.clone()),
                move |identities, active| {
                    let identities: Vec<_> = identities
                        .into_iter()
                        .filter(|g| g.deleted_at_ms == 0 && g.goal.project_id == id)
                        .collect();
                    let goals = identities.iter().map(|g| g.goal.clone()).collect();
                    view_with_metadata(id.clone(), goals, active.as_str(), &identities)
                },
            )
            .distinct_until_changed()
            .box_it()
    })
    .distinct_until_changed()
    .box_it();
    source::switch_map(checklist, tracked_context)
        .distinct_until_changed()
        .box_it()
}
fn tracked_context(vm: GoalBarVm) -> Observable<GoalBarVm> {
    if vm.active.is_empty() {
        return source::once(vm);
    }
    let current_key = vm
        .context_keys
        .last()
        .cloned()
        .unwrap_or_else(|| vm.active.clone());
    source::combine_latest(
        vm.context_keys
            .iter()
            .map(|key| source::proj::goal_title(key.clone()))
            .collect(),
    )
    .combine_latest(
        source::proj::goal_description(current_key),
        move |titles, description| {
            let mut vm = vm.clone();
            vm.title = titles
                .last()
                .cloned()
                .unwrap_or_else(|| "Choose goal".into());
            vm.description = description;
            vm.tooltip = tooltip(&titles.join(" › "), &vm.description);
            vm
        },
    )
    .distinct_until_changed()
    .box_it()
}
#[cfg(test)]
fn view(project: String, goals: Vec<GoalInfo>, active: &str) -> GoalBarVm {
    view_with_metadata(project, goals, active, &[])
}
fn view_with_metadata(
    project: String,
    goals: Vec<GoalInfo>,
    active: &str,
    identities: &[GoalIdentityInfo],
) -> GoalBarVm {
    let selected = goals.iter().find(|g| g.object_path() == active);
    let mut context = Vec::new();
    let mut current = selected;
    while let Some(goal) = current {
        if context.iter().any(|g: &&GoalInfo| g.key() == goal.key()) {
            break;
        }
        context.push(goal);
        current = goals.iter().find(|g| g.key() == goal.parent_id);
    }
    context.reverse();
    while let Some(goal) = context.last() {
        let mut in_progress = goals
            .iter()
            .filter(|g| g.parent_id == goal.key() && g.status == "in-progress");
        let Some(child) = in_progress.next() else {
            break;
        };
        if in_progress.next().is_some() || context.iter().any(|g| g.key() == child.key()) {
            break;
        }
        context.push(child);
    }
    let mut result = GoalBarVm {
        project_id: Some(project),
        title: context
            .last()
            .map(|g| g.title.clone())
            .unwrap_or_else(|| "Choose goal".into()),
        description: context
            .last()
            .map(|g| g.description.clone())
            .unwrap_or_else(|| "Project goals and subtasks".into()),
        active: selected.map(|g| g.key()).unwrap_or_default(),
        tooltip: String::new(),
        context_keys: context.iter().map(|g| g.key()).collect(),
        rows: catalog::rows(&goals, identities),
    };
    let breadcrumb = context
        .iter()
        .map(|g| g.title.as_str())
        .collect::<Vec<_>>()
        .join(" › ");
    result.tooltip = tooltip(
        if breadcrumb.is_empty() {
            &result.title
        } else {
            &breadcrumb
        },
        &result.description,
    );
    result
}
fn tooltip(title: &str, description: &str) -> String {
    if description.is_empty() {
        title.to_owned()
    } else {
        format!("{title}\n\n{description}")
    }
}

#[cfg(test)]
mod test;
