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

/// Flattens a project's goals into a displayable, depth-ordered task list.
/// Descendants of collapsed parents are omitted. The boolean flag indicates
/// whether the task has any children.
pub(crate) fn task_rows<'a>(
    tasks: &'a [GoalIdentityInfo],
    show_finished: bool,
    collapsed: &HashSet<String>,
) -> Vec<(&'a GoalIdentityInfo, usize, bool)> {
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

    fn walk<'a>(
        all: &[&'a GoalIdentityInfo],
        parent: &str,
        depth: usize,
        show: bool,
        collapsed: &HashSet<String>,
        out: &mut Vec<(&'a GoalIdentityInfo, usize, bool)>,
    ) {
        for task in all.iter().filter(|task| task.goal.parent_id == parent) {
            if out
                .iter()
                .any(|(seen, _, _)| seen.instance_id == task.instance_id)
            {
                continue;
            }
            if show
                || !is_finished(&task.goal.status)
                || all.iter().any(|child| {
                    child.goal.parent_id == task.key && !is_finished(&child.goal.status)
                })
            {
                let has_children = all.iter().any(|child| child.goal.parent_id == task.key);
                out.push((task, depth, has_children));
                if has_children && !collapsed.contains(&task.key) {
                    walk(all, &task.key, depth + 1, show, collapsed, out);
                }
            }
        }
    }

    let keys: HashSet<_> = tasks.iter().map(|task| task.key.as_str()).collect();
    let mut out = Vec::new();
    walk(&tasks, "", 0, show_finished, collapsed, &mut out);
    for task in tasks {
        if out
            .iter()
            .any(|(seen, _, _)| seen.instance_id == task.instance_id)
        {
            continue;
        }
        if show_finished || !is_finished(&task.goal.status) {
            let parent_missing =
                task.goal.parent_id.is_empty() || !keys.contains(task.goal.parent_id.as_str());
            if parent_missing {
                out.push((task, 0, false));
            }
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

    #[test]
    fn subtasks_are_ordered_under_their_parent_with_depth() {
        let tasks = vec![
            task("child", "child", "parent", "planned"),
            task("parent", "parent", "", "planned"),
            task("grandchild", "grandchild", "child", "planned"),
        ];
        let rows = task_rows(&tasks, false, &HashSet::new());
        let summary: Vec<_> = rows
            .iter()
            .map(|(task, depth, has_children)| (task.key.as_str(), *depth, *has_children))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("parent", 0, true),
                ("child", 1, true),
                ("grandchild", 2, false)
            ]
        );
    }

    #[test]
    fn collapsed_parent_hides_descendants() {
        let tasks = vec![
            task("parent", "parent", "", "planned"),
            task("child", "child", "parent", "planned"),
            task("grandchild", "grandchild", "child", "planned"),
        ];
        let collapsed = HashSet::from(["parent".to_string()]);
        let rows = task_rows(&tasks, false, &collapsed);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0.key, "parent");
    }

    #[test]
    fn finished_subtasks_follow_show_finished() {
        let tasks = vec![
            task("parent", "parent", "", "planned"),
            task("done", "done", "parent", "completed"),
        ];
        assert_eq!(task_rows(&tasks, false, &HashSet::new()).len(), 1);
        assert_eq!(task_rows(&tasks, true, &HashSet::new()).len(), 2);
    }
}
