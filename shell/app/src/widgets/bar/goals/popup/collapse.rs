use super::GoalRow;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct CollapseState {
    entries: HashMap<String, Entry>,
}
struct Entry {
    expanded: bool,
    completed: bool,
}
impl CollapseState {
    pub fn sync(&mut self, rows: &[GoalRow]) {
        for row in rows.iter().filter(|r| r.child_count > 0) {
            let entry = self
                .entries
                .entry(row.instance_id.clone())
                .or_insert(Entry {
                    expanded: !row.completed,
                    completed: row.completed,
                });
            if entry.completed != row.completed {
                entry.expanded = !row.completed;
                entry.completed = row.completed;
            }
        }
    }
    pub fn expanded(&self, row: &GoalRow) -> bool {
        self.entries
            .get(&row.instance_id)
            .map(|e| e.expanded)
            .unwrap_or(!row.completed)
    }
    pub fn toggle(&mut self, row: &GoalRow) {
        let expanded = self.expanded(row);
        self.entries.insert(
            row.instance_id.clone(),
            Entry {
                expanded: !expanded,
                completed: row.completed,
            },
        );
    }
    pub fn visibility(&self, rows: &[GoalRow]) -> Vec<bool> {
        let mut parents = Vec::new();
        let mut visible = Vec::new();
        for row in rows {
            parents.truncate(row.depth);
            visible.push(parents.iter().all(|expanded| *expanded));
            if row.child_count > 0 {
                parents.push(self.expanded(row));
            }
        }
        visible
    }
}

#[cfg(test)]
mod test;
