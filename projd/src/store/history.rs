use super::{Store, read};
use proj_model::{GoalEventInfo, GoalIdentityInfo, GoalInfo, GoalTrackingInfo, ProjectInfo};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

pub(super) fn schema(db: &mut Connection) -> anyhow::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS goal_instances(instance_id TEXT PRIMARY KEY,key TEXT NOT NULL,goal_json TEXT NOT NULL,project_name TEXT NOT NULL,created_at_ms INTEGER NOT NULL,observed_at_ms INTEGER NOT NULL,deleted_at_ms INTEGER NOT NULL DEFAULT 0,ancestor_ids TEXT NOT NULL);
        CREATE UNIQUE INDEX IF NOT EXISTS live_goal_key ON goal_instances(key) WHERE deleted_at_ms=0;
        CREATE TABLE IF NOT EXISTS goal_events(sequence INTEGER PRIMARY KEY AUTOINCREMENT,at_ms INTEGER NOT NULL,kind TEXT NOT NULL,instance_id TEXT NOT NULL,key TEXT NOT NULL,old_status TEXT NOT NULL,goal_json TEXT NOT NULL);
        CREATE INDEX IF NOT EXISTS goal_event_instance ON goal_events(instance_id,at_ms);
        CREATE TABLE IF NOT EXISTS goal_work(id TEXT PRIMARY KEY,owner TEXT NOT NULL,json TEXT NOT NULL);
        CREATE INDEX IF NOT EXISTS open_work_owner ON goal_work(owner) WHERE json_extract(json,'$.open')=1;")?;
    db.execute_batch("CREATE INDEX IF NOT EXISTS work_start_time ON goal_work(json_extract(json,'$.started_at_ms')); CREATE INDEX IF NOT EXISTS work_project_time ON goal_work(json_extract(json,'$.project_id'),json_extract(json,'$.started_at_ms'));")?;
    Ok(())
}

fn identities(db: &Connection) -> anyhow::Result<Vec<GoalIdentityInfo>> {
    let mut s = db.prepare("SELECT instance_id,key,goal_json,project_name,created_at_ms,observed_at_ms,deleted_at_ms,ancestor_ids FROM goal_instances ORDER BY observed_at_ms,instance_id")?;
    let rows = s.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, i64>(4)? as u64,
            r.get::<_, i64>(5)? as u64,
            r.get::<_, i64>(6)? as u64,
            r.get::<_, String>(7)?,
        ))
    })?;
    rows.map(|r| {
        let (
            instance_id,
            key,
            json,
            project_name,
            created_at_ms,
            observed_at_ms,
            deleted_at_ms,
            ancestors,
        ) = r?;
        Ok(GoalIdentityInfo {
            instance_id,
            key,
            goal: serde_json::from_str(&json)?,
            project_name,
            created_at_ms,
            observed_at_ms,
            deleted_at_ms,
            ancestor_ids: serde_json::from_str(&ancestors)?,
        })
    })
    .collect()
}

fn instance(db: &Connection, key: &str) -> anyhow::Result<Option<String>> {
    Ok(db
        .query_row(
            "SELECT instance_id FROM goal_instances WHERE key=? AND deleted_at_ms=0",
            [key],
            |r| r.get(0),
        )
        .optional()?)
}

pub(super) fn record(
    tx: &Transaction<'_>,
    goal: &GoalInfo,
    kind: &str,
    old_status: &str,
    at: u64,
) -> anyhow::Result<String> {
    let at = i64::try_from(at)?;
    let key = goal.key();
    let instance_id = if let Some(id) = instance(tx, &key)? {
        id
    } else {
        let id = uuid::Uuid::new_v4().simple().to_string();
        let projects: Vec<ProjectInfo> = read(tx, "project")?;
        let name = projects
            .iter()
            .find(|p| p.id == goal.project_id)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let ancestors = if goal.parent_id.is_empty() {
            Vec::new()
        } else {
            let parent = identities(tx)?
                .into_iter()
                .find(|p| p.key == goal.parent_id && p.deleted_at_ms == 0)
                .ok_or_else(|| anyhow::anyhow!("Parent history identity not found"))?;
            let mut ancestors = parent.ancestor_ids;
            ancestors.push(parent.instance_id);
            ancestors
        };
        tx.execute(
            "INSERT INTO goal_instances VALUES(?,?,?,?,?,?,0,?)",
            params![
                id,
                key,
                serde_json::to_string(goal)?,
                name,
                if kind == "created" { at } else { 0 },
                at,
                serde_json::to_string(&ancestors)?
            ],
        )?;
        id
    };
    tx.execute("INSERT INTO goal_events(at_ms,kind,instance_id,key,old_status,goal_json) VALUES(?,?,?,?,?,?)", params![at,kind,instance_id,key,old_status,serde_json::to_string(goal)?])?;
    let projects: Vec<ProjectInfo> = read(tx, "project")?;
    let name = projects
        .iter()
        .find(|p| p.id == goal.project_id)
        .map(|p| p.name.clone())
        .unwrap_or(tx.query_row(
            "SELECT project_name FROM goal_instances WHERE instance_id=?",
            [&instance_id],
            |r| r.get::<_, String>(0),
        )?);
    tx.execute(
        "UPDATE goal_instances SET goal_json=?,deleted_at_ms=?,project_name=? WHERE instance_id=?",
        params![
            serde_json::to_string(goal)?,
            if kind == "deleted" { at } else { 0 },
            name,
            instance_id
        ],
    )?;
    Ok(instance_id)
}

#[cfg(test)]
mod test;

impl Store {
    pub fn initialize_history(&self, at: u64) -> anyhow::Result<()> {
        let mut db = self.db.lock().unwrap();
        let mut pending: Vec<GoalInfo> = read(&db, "goal")?;
        let tx = db.transaction()?;
        while !pending.is_empty() {
            let mut progress = false;
            let mut rest = Vec::new();
            for goal in pending {
                if instance(&tx, &goal.key())?.is_some() {
                    progress = true;
                    continue;
                }
                if !goal.parent_id.is_empty() && instance(&tx, &goal.parent_id)?.is_none() {
                    rest.push(goal);
                    continue;
                }
                record(&tx, &goal, "observed", "", at)?;
                progress = true;
            }
            anyhow::ensure!(progress, "Goal history has a missing or cyclic parent");
            pending = rest;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn save_goal(&self, goal: &GoalInfo, create: bool, at: u64) -> anyhow::Result<bool> {
        let mut db = self.db.lock().unwrap();
        let old: Option<GoalInfo> = read::<GoalInfo>(&db, "goal")?
            .into_iter()
            .find(|g| g.key() == goal.key());
        if create && old.is_some() {
            return Ok(false);
        }
        anyhow::ensure!(create || old.is_some(), "Goal not found");
        if old.as_ref() == Some(goal) {
            return Ok(true);
        }
        let tx = db.transaction()?;
        tx.execute("INSERT INTO records(kind,id,json) VALUES('goal',?,?) ON CONFLICT(kind,id) DO UPDATE SET json=excluded.json",params![goal.key(),serde_json::to_string(goal)?])?;
        let kind = if create {
            "created"
        } else if old.as_ref().unwrap().status != goal.status {
            "status-changed"
        } else {
            "updated"
        };
        record(
            &tx,
            goal,
            kind,
            old.as_ref().map(|g| g.status.as_str()).unwrap_or(""),
            at,
        )?;
        tx.commit()?;
        Ok(true)
    }

    pub fn delete_goal_record(&self, goal: &GoalInfo, at: u64) -> anyhow::Result<()> {
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction()?;
        // Unselect before deleting the current identity so the selection event shares its lifetime.
        let mut selections = tx.prepare("SELECT id,json FROM records WHERE kind='active-goal'")?;
        let rows = selections
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(selections);
        for (project, json) in rows {
            if serde_json::from_str::<String>(&json)? == goal.key() {
                record(&tx, goal, "unselected", &goal.status, at)?;
                tx.execute(
                    "DELETE FROM records WHERE kind='active-goal' AND id=?",
                    [project],
                )?;
            }
        }
        record(&tx, goal, "deleted", &goal.status, at)?;
        tx.execute(
            "DELETE FROM records WHERE kind='goal' AND id=?",
            [goal.key()],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn select_goal(&self, project: &str, key: &str, at: u64) -> anyhow::Result<()> {
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction()?;
        let old = tx
            .query_row(
                "SELECT json FROM records WHERE kind='active-goal' AND id=?",
                [project],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .map(|s| serde_json::from_str::<String>(&s))
            .transpose()?
            .unwrap_or_default();
        if old == key {
            return Ok(());
        }
        let goals: Vec<GoalInfo> = read(&tx, "goal")?;
        if let Some(g) = goals.iter().find(|g| g.key() == old) {
            record(&tx, g, "unselected", &g.status, at)?;
        }
        if let Some(g) = goals.iter().find(|g| g.key() == key) {
            record(&tx, g, "selected", &g.status, at)?;
        }
        tx.execute("INSERT INTO records(kind,id,json) VALUES('active-goal',?,?) ON CONFLICT(kind,id) DO UPDATE SET json=excluded.json",params![project,serde_json::to_string(key)?])?;
        tx.commit()?;
        Ok(())
    }

    pub fn identities(&self) -> anyhow::Result<Vec<GoalIdentityInfo>> {
        identities(&self.db.lock().unwrap())
    }

    pub fn tracking(&self) -> anyhow::Result<Vec<GoalTrackingInfo>> {
        let identities = self.identities()?;
        let projects = self.projects()?;
        let mut result = Vec::new();
        for identity in identities.into_iter().filter(|g| g.deleted_at_ms == 0) {
            let g = identity.goal;
            result.push(GoalTrackingInfo {
                instance_id: identity.instance_id,
                key: identity.key.clone(),
                title: g.title,
                project_id: g.project_id.clone(),
                project_name: projects
                    .iter()
                    .find(|p| p.id == g.project_id)
                    .map(|p| p.name.clone())
                    .unwrap_or(identity.project_name),
                status: g.status,
                parent_instance_id: identity.ancestor_ids.last().cloned().unwrap_or_default(),
                selected: self.active_goal(&g.project_id)? == identity.key,
                ancestor_ids: identity.ancestor_ids,
            });
        }
        Ok(result)
    }

    pub fn history(&self, id: &str, from: u64, to: u64) -> anyhow::Result<Vec<GoalEventInfo>> {
        let db = self.db.lock().unwrap();
        let mut s=db.prepare("SELECT sequence,at_ms,kind,instance_id,key,old_status,goal_json FROM goal_events WHERE (instance_id=? OR ?='') AND at_ms>=? AND at_ms<? ORDER BY at_ms,sequence")?;
        let rows = s.query_map(
            params![id, id, i64::try_from(from)?, i64::try_from(to)?],
            |r| {
                Ok((
                    r.get::<_, i64>(0)? as u64,
                    r.get::<_, i64>(1)? as u64,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                ))
            },
        )?;
        rows.map(|r| {
            let (sequence, at_ms, kind, instance_id, key, old_status, json) = r?;
            Ok(GoalEventInfo {
                sequence,
                at_ms,
                kind,
                instance_id,
                key,
                old_status,
                goal: serde_json::from_str(&json)?,
            })
        })
        .collect()
    }
}
