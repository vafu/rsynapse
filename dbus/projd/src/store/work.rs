use super::Store;
use proj_model::WorkSessionInfo;
use rusqlite::params;

impl Store {
    pub fn work_session(&self, id: &str) -> anyhow::Result<Option<(String, WorkSessionInfo)>> {
        use rusqlite::OptionalExtension;
        let row = self
            .db
            .lock()
            .unwrap()
            .query_row("SELECT owner,json FROM goal_work WHERE id=?", [id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .optional()?;
        row.map(|(owner, json)| Ok((owner, serde_json::from_str(&json)?)))
            .transpose()
    }
    pub fn open_work(&self, owner: &str) -> anyhow::Result<Vec<(String, WorkSessionInfo)>> {
        let db = self.db.lock().unwrap();
        let mut s=db.prepare("SELECT owner,json FROM goal_work WHERE json_extract(json,'$.open')=1 AND (owner=? OR ?='')")?;
        let rows = s.query_map(params![owner, owner], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        rows.map(|r| {
            let (owner, json) = r?;
            Ok((owner, serde_json::from_str(&json)?))
        })
        .collect()
    }
    pub fn save_work(&self, owner: &str, session: &WorkSessionInfo) -> anyhow::Result<()> {
        self.db.lock().unwrap().execute(
            "INSERT INTO goal_work VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET json=excluded.json",
            params![session.id, owner, serde_json::to_string(session)?],
        )?;
        Ok(())
    }
    pub fn query_work(
        &self,
        goal: &str,
        project: &str,
        from: u64,
        to: u64,
        descendants: bool,
    ) -> anyhow::Result<Vec<WorkSessionInfo>> {
        let db = self.db.lock().unwrap();
        let mut s=db.prepare("SELECT json FROM goal_work WHERE (?='' OR json_extract(json,'$.instance_id')=? OR (? AND EXISTS(SELECT 1 FROM json_each(goal_work.json,'$.ancestor_ids') WHERE value=?))) AND (?='' OR json_extract(json,'$.project_id')=?) AND json_extract(json,'$.started_at_ms')<=? AND (json_extract(json,'$.open')=1 OR json_extract(json,'$.ended_at_ms')>=?) ORDER BY json_extract(json,'$.started_at_ms'),id")?;
        let rows = s.query_map(
            params![
                goal,
                goal,
                descendants,
                goal,
                project,
                project,
                i64::try_from(to)?,
                i64::try_from(from)?
            ],
            |r| r.get::<_, String>(0),
        )?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    pub fn interrupt_open_work(&self, at: u64) -> anyhow::Result<()> {
        for (owner, mut s) in self.open_work("")? {
            if !s.open {
                continue;
            }
            s.open = false;
            s.ended_at_ms = at.max(s.started_at_ms);
            s.duration_ms = 0;
            s.end_reason = "recorder-restarted-unknown-duration".into();
            self.save_work(&owner, &s)?;
        }
        Ok(())
    }
}
