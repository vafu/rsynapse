//! View-model helpers for tasks and notes.
use std::collections::HashSet;

use notebook_api::note::Note;
use proj_model::GoalIdentityInfo;

pub(crate) fn task_tag(instance: &str) -> String {
    task_notes::task_tag(instance)
}

pub(crate) fn main_note_tag(instance: &str) -> String {
    task_notes::main_note_tag(instance)
}

pub(crate) fn is_main_note(note: &Note, instance: &str) -> bool {
    task_notes::is_main_note(&note.tags, instance)
}

pub(crate) fn task_title(title: &str) -> &str {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        "Untitled"
    } else {
        trimmed
    }
}

pub(crate) fn is_finished(status: &str) -> bool {
    matches!(status, "completed" | "cancelled")
}

/// Sort order for the task tree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TaskSort {
    #[default]
    Status,
    Created,
    Priority,
}

impl TaskSort {
    pub(crate) const ALL: [Self; 3] = [Self::Status, Self::Created, Self::Priority];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Status => "Status",
            Self::Created => "Date created",
            Self::Priority => "Priority",
        }
    }

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Created => "created",
            Self::Priority => "priority",
        }
    }

    pub(crate) fn icon(self) -> &'static str {
        match self {
            Self::Status => "view-list-symbolic",
            Self::Created => "x-office-calendar-symbolic",
            Self::Priority => "starred-symbolic",
        }
    }
}

fn status_rank(status: &str) -> u8 {
    match status {
        "in-progress" => 0,
        "planned" => 1,
        "deferred" => 2,
        _ => 3,
    }
}

fn priority_rank(priority: &str) -> u8 {
    match priority {
        "high" => 0,
        "low" => 2,
        _ => 1,
    }
}

fn matches_query(task: &GoalIdentityInfo, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let hay = format!(
        "{} {} {} {}",
        task.goal.title, task.goal.description, task.goal.status, task.goal.priority
    )
    .to_lowercase();
    hay.contains(query)
}

pub(crate) fn parse_tags(text: &str) -> Vec<String> {
    let mut tags: Vec<_> = text
        .split(',')
        .map(|tag| tag.trim().trim_start_matches('#'))
        .filter(|tag| !tag.is_empty())
        .map(str::to_owned)
        .collect();
    tags.sort();
    tags.dedup();
    tags
}

/// A displayable task with its visible subtasks nested underneath it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TaskNode {
    pub instance_id: String,
    pub key: String,
    pub project_id: String,
    pub depth: usize,
    pub has_children: bool,
    pub children: Vec<TaskNode>,
}

/// Builds a project's task tree in display order. Finished tasks are omitted
/// unless requested or they still have visible descendants.
pub(crate) fn task_nodes(
    tasks: &[GoalIdentityInfo],
    show_finished: bool,
    active: &[String],
    query: &str,
    sort: TaskSort,
) -> Vec<TaskNode> {
    let matching_keys: HashSet<&str> = tasks
        .iter()
        .filter(|task| task.deleted_at_ms == 0 && matches_query(task, query))
        .map(|task| task.key.as_str())
        .collect();
    let mut visible_keys = matching_keys.clone();
    if !query.is_empty() {
        for task in tasks
            .iter()
            .filter(|task| task.deleted_at_ms == 0 && matching_keys.contains(task.key.as_str()))
        {
            let mut parent = task.goal.parent_id.as_str();
            while !parent.is_empty() && visible_keys.insert(parent) {
                match tasks.iter().find(|candidate| candidate.key == parent) {
                    Some(found) => parent = found.goal.parent_id.as_str(),
                    None => break,
                }
            }
        }
    }
    let mut tasks: Vec<_> = tasks
        .iter()
        .filter(|task| task.deleted_at_ms == 0)
        .filter(|task| query.is_empty() || visible_keys.contains(task.key.as_str()))
        .collect();
    tasks.sort_by(|a, b| {
        let primary = match sort {
            TaskSort::Status => status_rank(&a.goal.status).cmp(&status_rank(&b.goal.status)),
            TaskSort::Created => a.created_at_ms.cmp(&b.created_at_ms),
            TaskSort::Priority => {
                priority_rank(&a.goal.priority).cmp(&priority_rank(&b.goal.priority))
            }
        };
        primary
            .then_with(|| a.created_at_ms.cmp(&b.created_at_ms))
            .then_with(|| a.instance_id.cmp(&b.instance_id))
    });

    fn build(
        task: &GoalIdentityInfo,
        all: &[&GoalIdentityInfo],
        depth: usize,
        show: bool,
        active: &[String],
        visited: &mut HashSet<String>,
    ) -> Option<TaskNode> {
        if !visited.insert(task.instance_id.clone()) {
            return None;
        }
        let children = all
            .iter()
            .filter(|child| child.goal.parent_id == task.key)
            .filter_map(|child| build(child, all, depth + 1, show, active, visited))
            .collect::<Vec<_>>();
        if !show && is_finished(&task.goal.status) && children.is_empty() {
            return None;
        }
        Some(TaskNode {
            instance_id: task.instance_id.clone(),
            key: task.key.clone(),
            project_id: task.goal.project_id.clone(),
            depth,
            has_children: !children.is_empty(),
            children,
        })
    }

    let keys: HashSet<_> = tasks.iter().map(|task| task.key.clone()).collect();
    let mut out = Vec::new();
    let mut visited = HashSet::new();
    for task in &tasks {
        let parent_missing = task.goal.parent_id.is_empty()
            || !keys.contains(&task.goal.parent_id)
            || task.goal.parent_id == task.key;
        if parent_missing
            && let Some(node) = build(task, &tasks, 0, show_finished, active, &mut visited)
        {
            out.push(node);
        }
    }
    for task in &tasks {
        if let Some(node) = build(task, &tasks, 0, show_finished, active, &mut visited) {
            out.push(node);
        }
    }
    out
}

#[cfg(test)]
mod test {
    use super::*;
    use proj_model::{GoalIdentityInfo, GoalInfo};

    fn task(instance: &str, key: &str, parent: &str, status: &str) -> GoalIdentityInfo {
        GoalIdentityInfo {
            instance_id: instance.into(),
            key: key.into(),
            goal: GoalInfo {
                id: key.into(),
                title: key.into(),
                status: status.into(),
                parent_id: parent.into(),
                ..Default::default()
            },
            project_name: String::new(),
            created_at_ms: 0,
            observed_at_ms: 0,
            deleted_at_ms: 0,
            ancestor_ids: Vec::new(),
        }
    }

    fn summary(nodes: &[TaskNode]) -> Vec<String> {
        fn branch(node: &TaskNode) -> String {
            if node.children.is_empty() {
                node.key.clone()
            } else {
                format!(
                    "{}[{}]",
                    node.key,
                    node.children
                        .iter()
                        .map(branch)
                        .collect::<Vec<_>>()
                        .join(",")
                )
            }
        }
        nodes.iter().map(branch).collect()
    }

    #[test]
    fn subtasks_are_nested_under_their_parent() {
        let tasks = vec![
            task("child", "child", "parent", "planned"),
            task("parent", "parent", "", "planned"),
            task("grandchild", "grandchild", "child", "planned"),
        ];
        let nodes = task_nodes(&tasks, false, &[], "", TaskSort::Status);
        assert_eq!(summary(&nodes), vec!["parent[child[grandchild]]"]);
    }

    #[test]
    fn finished_parent_stays_visible_when_it_has_open_descendants() {
        let tasks = vec![
            task("parent", "parent", "", "completed"),
            task("child", "child", "parent", "planned"),
        ];
        let nodes = task_nodes(&tasks, false, &[], "", TaskSort::Status);
        assert_eq!(summary(&nodes), vec!["parent[child]"]);
    }

    #[test]
    fn finished_subtasks_follow_show_finished() {
        let tasks = vec![
            task("parent", "parent", "", "planned"),
            task("done", "done", "parent", "completed"),
        ];
        let hidden = task_nodes(&tasks, false, &[], "", TaskSort::Status);
        assert_eq!(hidden.len(), 1);
        assert!(hidden[0].children.is_empty());

        let shown = task_nodes(&tasks, true, &[], "", TaskSort::Status);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].children[0].key, "done");
    }

    fn titled(
        mut task: GoalIdentityInfo,
        title: &str,
        priority: &str,
        created: u64,
    ) -> GoalIdentityInfo {
        task.goal.title = title.into();
        task.goal.priority = priority.into();
        task.created_at_ms = created;
        task
    }

    #[test]
    fn search_keeps_matching_subtask_and_its_ancestors() {
        let tasks = vec![
            titled(
                task("parent", "parent", "", "planned"),
                "Alpha parent",
                "low",
                1,
            ),
            titled(
                task("child", "child", "parent", "planned"),
                "Beta child",
                "low",
                2,
            ),
        ];
        let nodes = task_nodes(&tasks, false, &[], "beta", TaskSort::Status);
        assert_eq!(summary(&nodes), vec!["parent[child]"]);

        let none = task_nodes(&tasks, false, &[], "gamma", TaskSort::Status);
        assert!(none.is_empty());
    }

    #[test]
    fn priority_sort_orders_high_before_medium_before_low() {
        let tasks = vec![
            titled(task("low", "low", "", "planned"), "Low", "low", 1),
            titled(task("high", "high", "", "planned"), "High", "high", 3),
            titled(task("medium", "medium", "", "planned"), "Med", "medium", 2),
        ];
        let nodes = task_nodes(&tasks, false, &[], "", TaskSort::Priority);
        assert_eq!(summary(&nodes), vec!["high", "medium", "low"]);
    }

    #[test]
    fn created_sort_orders_oldest_first() {
        let tasks = vec![
            titled(task("newer", "newer", "", "planned"), "Newer", "high", 30),
            titled(task("older", "older", "", "planned"), "Older", "low", 10),
        ];
        let nodes = task_nodes(&tasks, false, &[], "", TaskSort::Created);
        assert_eq!(summary(&nodes), vec!["older", "newer"]);
    }
}
