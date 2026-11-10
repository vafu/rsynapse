use proj_model::{CheckoutInfo, GoalInfo, ProjectInfo, RemovedProjectInfo};
use rusqlite::{Connection, params};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
};
mod history;
mod labels;
mod work;
pub struct Store {
    db: Mutex<Connection>,
}
pub struct RemovedProject {
    pub project: ProjectInfo,
    pub checkouts: Vec<CheckoutInfo>,
}
impl RemovedProject {
    pub fn info(&self) -> RemovedProjectInfo {
        RemovedProjectInfo {
            project: self.project.clone(),
            checkouts: self.checkouts.clone(),
        }
    }
}
fn read<T: DeserializeOwned>(db: &Connection, kind: &str) -> anyhow::Result<Vec<T>> {
    let mut stmt = db.prepare("SELECT json FROM records WHERE kind=? ORDER BY id")?;
    let rows = stmt.query_map([kind], |r| r.get::<_, String>(0))?;
    rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
}
pub fn default_path() -> PathBuf {
    if let Some(path) = std::env::var_os("PROJD_STORE_PATH") {
        return proj_model::xdg::absolute(PathBuf::from(path));
    }
    proj_model::xdg::state_home()
        .unwrap_or_else(|_| PathBuf::from(".local/state"))
        .join("rsynapse/projd/projects.sqlite3")
}
impl Store {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut db = Connection::open(path)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS records(kind TEXT NOT NULL,id TEXT NOT NULL,json TEXT NOT NULL,PRIMARY KEY(kind,id)); CREATE INDEX IF NOT EXISTS goal_parent_id ON records(json_extract(json,'$.parent_id')) WHERE kind='goal';")?;
        let legacy:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM records WHERE (kind='project' AND json_extract(json,'$.cwd') IS NULL) OR kind='context')",[],|r|r.get(0))?;
        if legacy {
            let backup = path.with_file_name(format!(
                "before-model-v2-{}.sqlite3",
                chrono::Utc::now().timestamp_micros()
            ));
            db.execute("VACUUM INTO ?", [backup.to_string_lossy().as_ref()])?;
        }
        crate::migrate::run(&mut db)?;
        history::schema(&mut db)?;
        let legacy_goals: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM records WHERE kind='goal' AND json_extract(json,'$.note_path') IS NULL)", [], |r| r.get(0))?;
        if legacy_goals {
            let backup = path.with_file_name(format!(
                "before-goal-checklists-{}.sqlite3",
                chrono::Utc::now().timestamp_micros()
            ));
            db.execute("VACUUM INTO ?", [backup.to_string_lossy().as_ref()])?;
        }
        Ok(Self { db: Mutex::new(db) })
    }
    pub fn list<T: DeserializeOwned>(&self, kind: &str) -> anyhow::Result<Vec<T>> {
        read(&self.db.lock().unwrap(), kind)
    }
    pub fn put<T: Serialize>(&self, kind: &str, id: &str, value: &T) -> anyhow::Result<()> {
        self.db.lock().unwrap().execute("INSERT INTO records(kind,id,json) VALUES(?,?,?) ON CONFLICT(kind,id) DO UPDATE SET json=excluded.json",params![kind,id,serde_json::to_string(value)?])?;
        Ok(())
    }
    pub fn get<T: DeserializeOwned>(&self, kind: &str, id: &str) -> anyhow::Result<Option<T>> {
        let db = self.db.lock().unwrap();
        let mut statement = db.prepare("SELECT json FROM records WHERE kind=? AND id=?")?;
        let mut rows = statement.query(params![kind, id])?;
        Ok(match rows.next()? {
            Some(row) => Some(serde_json::from_str(&row.get::<_, String>(0)?)?),
            None => None,
        })
    }
    pub fn goal_by_key(&self, key: &str) -> anyhow::Result<Option<GoalInfo>> {
        let (date, id) = key.split_once('/').unwrap_or(("", key));
        Ok(self
            .get::<GoalInfo>("goal", id)?
            .filter(|goal| goal.date == date && goal.key() == key))
    }
    pub fn children_of(&self, key: &str) -> anyhow::Result<Vec<GoalInfo>> {
        let db = self.db.lock().unwrap();
        let mut statement = db.prepare(
            "SELECT json FROM records WHERE kind='goal' AND json_extract(json,'$.parent_id')=? ORDER BY id",
        )?;
        let rows = statement.query_map([key], |row| row.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }
    #[cfg(test)]
    pub fn create<T: Serialize>(&self, kind: &str, id: &str, value: &T) -> anyhow::Result<bool> {
        Ok(self.db.lock().unwrap().execute(
            "INSERT OR IGNORE INTO records(kind,id,json) VALUES(?,?,?)",
            params![kind, id, serde_json::to_string(value)?],
        )? == 1)
    }
    #[cfg(test)]
    pub fn remove(&self, kind: &str, id: &str) -> anyhow::Result<bool> {
        Ok(self.db.lock().unwrap().execute(
            "DELETE FROM records WHERE kind=? AND id=?",
            params![kind, id],
        )? != 0)
    }
    pub fn projects(&self) -> anyhow::Result<Vec<ProjectInfo>> {
        self.list("project")
    }
    pub fn put_project(
        &self,
        project: &ProjectInfo,
        checkout: Option<&CheckoutInfo>,
    ) -> anyhow::Result<()> {
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction()?;
        if let Some(checkout) = checkout {
            tx.execute("INSERT INTO records(kind,id,json) VALUES('checkout',?,?) ON CONFLICT(kind,id) DO UPDATE SET json=excluded.json", params![checkout.id, serde_json::to_string(checkout)?])?;
        }
        tx.execute("INSERT INTO records(kind,id,json) VALUES('project',?,?) ON CONFLICT(kind,id) DO UPDATE SET json=excluded.json", params![project.id, serde_json::to_string(project)?])?;
        tx.commit()?;
        Ok(())
    }
    pub fn checkouts(&self) -> anyhow::Result<Vec<CheckoutInfo>> {
        self.list("checkout")
    }
    pub fn goals(&self) -> anyhow::Result<Vec<GoalInfo>> {
        self.list("goal")
    }
    pub fn active_goal(&self, project: &str) -> anyhow::Result<String> {
        let db = self.db.lock().unwrap();
        let mut statement =
            db.prepare("SELECT json FROM records WHERE kind='active-goal' AND id=?")?;
        let mut rows = statement.query([project])?;
        Ok(match rows.next()? {
            Some(row) => serde_json::from_str(&row.get::<_, String>(0)?)?,
            None => String::new(),
        })
    }
    pub fn active_goals(&self) -> anyhow::Result<HashMap<String, String>> {
        let db = self.db.lock().unwrap();
        let mut statement = db.prepare("SELECT id,json FROM records WHERE kind='active-goal'")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut goals = HashMap::new();
        for row in rows {
            let (project, json) = row?;
            goals.insert(project, serde_json::from_str::<String>(&json)?);
        }
        Ok(goals)
    }
    pub fn remove_project(&self, id: &str) -> anyhow::Result<Option<RemovedProject>> {
        let mut db = self.db.lock().unwrap();
        let Some(project) = read::<ProjectInfo>(&db, "project")?
            .into_iter()
            .find(|p| p.id == id)
        else {
            return Ok(None);
        };
        let removed = RemovedProject {
            project: project.clone(),
            checkouts: read::<CheckoutInfo>(&db, "checkout")?
                .into_iter()
                .filter(|c| c.id == project.checkout_id)
                .collect(),
        };
        let tx = db.transaction()?;
        let goals: Vec<GoalInfo> = read(&tx, "goal")?;
        let selected = tx.query_row(
            "SELECT json FROM records WHERE kind='active-goal' AND id=?",
            [id],
            |r| r.get::<_, String>(0),
        );
        if let Ok(selected) = selected {
            let key: String = serde_json::from_str(&selected)?;
            if let Some(goal) = goals.iter().find(|g| g.key() == key) {
                history::record(
                    &tx,
                    goal,
                    "unselected",
                    &goal.status,
                    crate::metrics::now_ms(),
                )?;
            }
        }
        for mut goal in goals {
            let references_removed = goal.project_id == id
                || (!goal.project.is_empty()
                    && (goal.project == project.cwd
                        || removed
                            .checkouts
                            .iter()
                            .any(|c| c.root_path == goal.project)));
            if references_removed {
                goal.project.clear();
                goal.project_id.clear();
                tx.execute(
                    "UPDATE records SET json=? WHERE kind='goal' AND id=?",
                    params![serde_json::to_string(&goal)?, goal.id],
                )?;
            }
        }
        for c in &removed.checkouts {
            tx.execute(
                "DELETE FROM records WHERE kind='checkout' AND id=?",
                [&c.id],
            )?;
        }
        tx.execute("DELETE FROM records WHERE kind='project' AND id=?", [id])?;
        tx.execute(
            "DELETE FROM records WHERE kind='project-labels' AND id=?",
            [id],
        )?;
        tx.execute(
            "DELETE FROM records WHERE kind='active-goal' AND id=?",
            [id],
        )?;
        tx.commit()?;
        Ok(Some(removed))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_removal_is_durable_and_preserves_goals_and_other_projects() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("store.db");
        let store = Store::open(&path).unwrap();
        let p = ProjectInfo {
            id: "p".into(),
            name: "Project".into(),
            cwd: "/project".into(),
            checkout_id: "c".into(),
            icon: String::new(),
            icon_origin: String::new(),
        };
        let other = ProjectInfo {
            id: "other".into(),
            cwd: "/other".into(),
            checkout_id: String::new(),
            ..p.clone()
        };
        let c = CheckoutInfo {
            id: "c".into(),
            root_path: "/checkout".into(),
            branch: "main".into(),
            git_status: Default::default(),
        };
        let goal = GoalInfo {
            id: "g".into(),
            date: "2026-10-08".into(),
            title: "History".into(),
            kind: "outcome".into(),
            project: "/checkout".into(),
            success: "Preserved".into(),
            priority: "high".into(),
            status: "planned".into(),
            ..Default::default()
        };
        store.put("project", "p", &p).unwrap();
        store.put("project", "other", &other).unwrap();
        store.put("checkout", "c", &c).unwrap();
        store.put("goal", "g", &goal).unwrap();
        assert_eq!(
            store.remove_project("p").unwrap().unwrap().checkouts,
            vec![c]
        );
        assert!(store.remove_project("p").unwrap().is_none());
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.projects().unwrap(), vec![other]);
        assert!(store.checkouts().unwrap().is_empty());
        let goals = store.goals().unwrap();
        assert_eq!(goals.len(), 1);
        assert_eq!(goals[0].id, goal.id);
        assert_eq!(goals[0].title, goal.title);
        assert!(goals[0].project.is_empty());
        assert!(goals[0].project_id.is_empty());
    }
    #[test]
    fn tracking_preloads_project_names_and_active_selection() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("store.db")).unwrap();
        let project = ProjectInfo {
            id: "p".into(),
            name: "Project".into(),
            cwd: "/p".into(),
            checkout_id: String::new(),
            icon: String::new(),
            icon_origin: String::new(),
        };
        let goal = GoalInfo {
            id: "g".into(),
            title: "Task".into(),
            kind: "task".into(),
            project_id: "p".into(),
            project: "/p".into(),
            status: "in-progress".into(),
            ..Default::default()
        };
        store.put("project", &project.id, &project).unwrap();
        store.put("goal", &goal.id, &goal).unwrap();
        store.initialize_history(1).unwrap();
        store.put("active-goal", "p", &goal.key()).unwrap();
        let tracking = store.tracking().unwrap();
        assert_eq!(tracking.len(), 1);
        assert_eq!(tracking[0].project_name, "Project");
        assert!(tracking[0].selected);
    }

    #[test]
    fn active_goals_reads_all_projects_once() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("store.db")).unwrap();
        store
            .put("active-goal", "alpha", &"2026-01-01/a".to_owned())
            .unwrap();
        store.put("active-goal", "beta", &"b".to_owned()).unwrap();
        let goals = store.active_goals().unwrap();
        assert_eq!(goals.get("alpha").map(String::as_str), Some("2026-01-01/a"));
        assert_eq!(goals.get("beta").map(String::as_str), Some("b"));
    }

    #[test]
    fn indexed_goal_lookup_and_children_respect_the_store_key() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("store.db")).unwrap();
        let parent = GoalInfo {
            id: "parent".into(),
            date: "2026-01-02".into(),
            title: "Parent".into(),
            kind: "task".into(),
            ..Default::default()
        };
        let child = GoalInfo {
            id: "child".into(),
            title: "Child".into(),
            kind: "task".into(),
            parent_id: parent.key(),
            ..Default::default()
        };
        store.put("goal", &parent.id, &parent).unwrap();
        store.put("goal", &child.id, &child).unwrap();

        assert_eq!(
            store.goal_by_key(&parent.key()).unwrap(),
            Some(parent.clone())
        );
        assert_eq!(store.goal_by_key("missing").unwrap(), None);
        assert_eq!(store.children_of(&parent.key()).unwrap(), vec![child]);
        assert_eq!(
            store.get::<GoalInfo>("goal", "parent").unwrap(),
            Some(parent)
        );
    }

    #[test]
    fn durable_records_survive_restart_and_duplicates_do_not_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("store.db");
        let p = ProjectInfo {
            id: "stable".into(),
            name: "Project".into(),
            cwd: "/project".into(),
            checkout_id: String::new(),
            icon: String::new(),
            icon_origin: String::new(),
        };
        let store = Store::open(&path).unwrap();
        assert!(store.create("project", "stable", &p).unwrap());
        assert!(
            !store
                .create(
                    "project",
                    "stable",
                    &ProjectInfo {
                        name: "Overwrite".into(),
                        ..p.clone()
                    }
                )
                .unwrap()
        );
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.projects().unwrap(), vec![p]);
        assert!(store.remove("project", "stable").unwrap());
    }
}
