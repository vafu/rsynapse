use super::{GoalIdentityInfo, GoalInfo, GoalRow};
use std::collections::{HashMap, HashSet};

fn priority(goal: &GoalInfo) -> u8 {
    match goal.priority.as_str() {
        "high" => 0,
        "low" => 2,
        _ => 1,
    }
}
fn completed(
    key: &str,
    goals: &[GoalInfo],
    memo: &mut HashMap<String, bool>,
    visiting: &mut HashSet<String>,
) -> bool {
    if let Some(done) = memo.get(key) {
        return *done;
    }
    if !visiting.insert(key.to_owned()) {
        return false;
    }
    let Some(goal) = goals.iter().find(|g| g.key() == key) else {
        return false;
    };
    let children: Vec<_> = goals.iter().filter(|g| g.parent_id == key).collect();
    let done = goal.status == "completed"
        || (!children.is_empty()
            && children
                .iter()
                .all(|g| completed(&g.key(), goals, memo, visiting)));
    visiting.remove(key);
    memo.insert(key.to_owned(), done);
    done
}

pub(super) fn rows(goals: &[GoalInfo], identities: &[GoalIdentityInfo]) -> Vec<GoalRow> {
    let mut done = HashMap::new();
    for g in goals {
        completed(&g.key(), goals, &mut done, &mut HashSet::new());
    }
    let metadata: HashMap<_, _> = identities
        .iter()
        .filter(|g| g.deleted_at_ms == 0)
        .map(|g| (g.key.clone(), g))
        .collect();
    fn children(
        parent: &str,
        depth: usize,
        goals: &[GoalInfo],
        done: &HashMap<String, bool>,
        metadata: &HashMap<String, &GoalIdentityInfo>,
        result: &mut Vec<GoalRow>,
    ) {
        let mut siblings: Vec<_> = goals.iter().filter(|g| g.parent_id == parent).collect();
        let created = |g: &GoalInfo| {
            metadata
                .get(&g.key())
                .map(|i| i.created_at_ms)
                .filter(|ms| *ms > 0)
                .unwrap_or(u64::MAX)
        };
        siblings.sort_by(|a, b| {
            done.get(&a.key())
                .cmp(&done.get(&b.key()))
                .then_with(|| priority(a).cmp(&priority(b)))
                .then_with(|| created(a).cmp(&created(b)))
                .then_with(|| a.key().cmp(&b.key()))
        });
        for g in siblings {
            let child_count = goals.iter().filter(|c| c.parent_id == g.key()).count();
            let completed_children = goals
                .iter()
                .filter(|c| c.parent_id == g.key() && done.get(&c.key()) == Some(&true))
                .count();
            let id = metadata.get(&g.key());
            result.push(GoalRow {
                goal: g.clone(),
                depth,
                completed: done.get(&g.key()) == Some(&true),
                child_count,
                completed_children,
                instance_id: id.map(|i| i.instance_id.clone()).unwrap_or_else(|| g.key()),
                created_at_ms: id.map(|i| i.created_at_ms).unwrap_or(0),
            });
            children(&g.key(), depth + 1, goals, done, metadata, result);
        }
    }
    let mut result = Vec::new();
    children("", 0, goals, &done, &metadata, &mut result);
    result
}

#[cfg(test)]
mod test;
