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
) -> Vec<TaskNode> {
    let mut tasks: Vec<_> = tasks
        .iter()
        .filter(|task| task.deleted_at_ms == 0)
        .collect();
    tasks.sort_by(|a, b| {
        let rank = |status: &str| match status {
            "in-progress" => 0,
            "planned" => 1,
            "deferred" => 2,
            _ => 3,
        };
        (rank(&a.goal.status), a.created_at_ms, &a.instance_id).cmp(&(
            rank(&b.goal.status),
            b.created_at_ms,
            &b.instance_id,
        ))
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
        let nodes = task_nodes(&tasks, false, &[]);
        assert_eq!(summary(&nodes), vec!["parent[child[grandchild]]"]);
    }

    #[test]
    fn finished_parent_stays_visible_when_it_has_open_descendants() {
        let tasks = vec![
            task("parent", "parent", "", "completed"),
            task("child", "child", "parent", "planned"),
        ];
        let nodes = task_nodes(&tasks, false, &[]);
        assert_eq!(summary(&nodes), vec!["parent[child]"]);
    }

    #[test]
    fn finished_subtasks_follow_show_finished() {
        let tasks = vec![
            task("parent", "parent", "", "planned"),
            task("done", "done", "parent", "completed"),
        ];
        let hidden = task_nodes(&tasks, false, &[]);
        assert_eq!(hidden.len(), 1);
        assert!(hidden[0].children.is_empty());

        let shown = task_nodes(&tasks, true, &[]);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].children[0].key, "done");
    }
}
