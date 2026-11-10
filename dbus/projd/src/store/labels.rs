use super::Store;
use rusqlite::{OptionalExtension, params};
use std::collections::BTreeSet;

fn read(db: &rusqlite::Connection, id: &str) -> anyhow::Result<Vec<String>> {
    let json: Option<String> = db
        .query_row(
            "SELECT json FROM records WHERE kind='project-labels' AND id=?",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(match json {
        Some(json) => serde_json::from_str(&json)?,
        None => Vec::new(),
    })
}

impl Store {
    pub fn project_labels(&self, id: &str) -> anyhow::Result<Vec<String>> {
        read(&self.db.lock().unwrap(), id)
    }

    // The caller validates/normalizes the whole batch before entering this method.
    // The transaction checks ownership and reads the latest set, never a ProjectInfo
    // snapshot, so metadata updates cannot overwrite labels or resurrect a project.
    pub fn edit_project_labels(
        &self,
        id: &str,
        labels: Vec<String>,
        add: bool,
    ) -> anyhow::Result<Option<(Vec<String>, bool)>> {
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM records WHERE kind='project' AND id=?)",
            [id],
            |r| r.get(0),
        )?;
        if !exists {
            return Ok(None);
        }
        let before = read(&tx, id)?;
        let mut result: BTreeSet<String> = before.iter().cloned().collect();
        for label in labels {
            if add {
                result.insert(label);
            } else {
                result.remove(&label);
            }
        }
        let result: Vec<String> = result.into_iter().collect();
        let changed = result != before;
        if changed {
            if result.is_empty() {
                tx.execute(
                    "DELETE FROM records WHERE kind='project-labels' AND id=?",
                    [id],
                )?;
            } else {
                tx.execute(
                    "INSERT INTO records(kind,id,json) VALUES('project-labels',?,?) ON CONFLICT(kind,id) DO UPDATE SET json=excluded.json",
                    params![id, serde_json::to_string(&result)?],
                )?;
            }
        }
        tx.commit()?;
        Ok(Some((result, changed)))
    }
}

#[cfg(test)]
mod tests;
