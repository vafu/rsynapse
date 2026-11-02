//! View-model helpers for tasks and notes.
use proj_model::GoalIdentityInfo;

pub(crate) fn task_tag(instance: &str) -> String {
    format!("task/{instance}")
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
pub(crate) fn task_rows<'a>(
    tasks: &'a [GoalIdentityInfo],
    show_finished: bool,
) -> Vec<(&'a GoalIdentityInfo, usize)> {
    let mut tasks: Vec<_> = tasks.iter().filter(|task| task.deleted_at_ms == 0).collect();
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
        out: &mut Vec<(&'a GoalIdentityInfo, usize)>,
    ) {
        for task in all.iter().filter(|task| task.goal.parent_id == parent) {
            if out
                .iter()
                .any(|(seen, _)| seen.instance_id == task.instance_id)
            {
                continue;
            }
            if show
                || !is_finished(&task.goal.status)
                || all.iter().any(|child| {
                    child.goal.parent_id == task.key && !is_finished(&child.goal.status)
                })
            {
                out.push((task, depth));
                walk(all, &task.key, depth + 1, show, out);
            }
        }
    }

    let mut out = Vec::new();
    walk(&tasks, "", 0, show_finished, &mut out);
    for task in tasks {
        if !out
            .iter()
            .any(|(seen, _)| seen.instance_id == task.instance_id)
            && (show_finished || !is_finished(&task.goal.status))
        {
            out.push((task, 0));
        }
    }
    out
}
