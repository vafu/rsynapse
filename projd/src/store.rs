use proj_model::{CheckoutInfo, ContextInfo, GoalInfo, ProjectInfo, RemovedProjectInfo};
use rusqlite::{Connection, params};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};
pub struct Store {
    db: Mutex<Connection>,
}
pub struct RemovedProject {
    pub project: ProjectInfo,
    pub checkouts: Vec<CheckoutInfo>,
    pub contexts: Vec<ContextInfo>,
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
    std::env::var_os("PROJD_STORE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("XDG_STATE_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(std::env::var_os("HOME").unwrap()).join(".local/state")
                })
                .join("rsynapse/projd/projects.sqlite3")
        })
}
impl Store {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let db = Connection::open(path)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS records(kind TEXT NOT NULL,id TEXT NOT NULL,json TEXT NOT NULL,PRIMARY KEY(kind,id));")?;
        Ok(Self { db: Mutex::new(db) })
    }
    pub fn list<T: DeserializeOwned>(&self, kind: &str) -> anyhow::Result<Vec<T>> {
        read(&self.db.lock().unwrap(), kind)
    }
    pub fn put<T: Serialize>(&self, kind: &str, id: &str, value: &T) -> anyhow::Result<()> {
        self.db.lock().unwrap().execute("INSERT INTO records(kind,id,json) VALUES(?,?,?) ON CONFLICT(kind,id) DO UPDATE SET json=excluded.json",params![kind,id,serde_json::to_string(value)?])?;
        Ok(())
    }
    pub fn create<T: Serialize>(&self, kind: &str, id: &str, value: &T) -> anyhow::Result<bool> {
        Ok(self.db.lock().unwrap().execute(
            "INSERT OR IGNORE INTO records(kind,id,json) VALUES(?,?,?)",
            params![kind, id, serde_json::to_string(value)?],
        )? == 1)
    }
    pub fn remove(&self, kind: &str, id: &str) -> anyhow::Result<bool> {
        Ok(self.db.lock().unwrap().execute(
            "DELETE FROM records WHERE kind=? AND id=?",
            params![kind, id],
        )? != 0)
    }
    pub fn projects(&self) -> anyhow::Result<Vec<ProjectInfo>> {
        self.list("project")
    }
    pub fn checkouts(&self) -> anyhow::Result<Vec<CheckoutInfo>> {
        self.list("checkout")
    }
    pub fn contexts(&self) -> anyhow::Result<Vec<ContextInfo>> {
        self.list("context")
    }
    pub fn goals(&self) -> anyhow::Result<Vec<GoalInfo>> {
        self.list("goal")
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
            project,
            checkouts: read::<CheckoutInfo>(&db, "checkout")?
                .into_iter()
                .filter(|c| c.project_id == id)
                .collect(),
            contexts: read::<ContextInfo>(&db, "context")?
                .into_iter()
                .filter(|c| c.project_id == id)
                .collect(),
        };
        let tx = db.transaction()?;
        for c in &removed.contexts {
            tx.execute("DELETE FROM records WHERE kind='context' AND id=?", [&c.id])?;
        }
        for c in &removed.checkouts {
            tx.execute(
                "DELETE FROM records WHERE kind='checkout' AND id=?",
                [&c.id],
            )?;
        }
        tx.execute("DELETE FROM records WHERE kind='project' AND id=?", [id])?;
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
            root_path: "/project".into(),
        };
        let other = ProjectInfo {
            id: "other".into(),
            root_path: "/other".into(),
            ..p.clone()
        };
        let c = CheckoutInfo {
            id: "c".into(),
            project_id: "p".into(),
            root_path: "/checkout".into(),
            branch: "main".into(),
            git_status: Default::default(),
        };
        let ctx = ContextInfo {
            id: "ctx".into(),
            project_id: "p".into(),
            checkout_id: "c".into(),
            cwd: "/checkout/subdir".into(),
            relative_cwd: "subdir".into(),
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
        };
        store.put("project", "p", &p).unwrap();
        store.put("project", "other", &other).unwrap();
        store.put("checkout", "c", &c).unwrap();
        store.put("context", "ctx", &ctx).unwrap();
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
        assert!(store.contexts().unwrap().is_empty());
        assert_eq!(store.goals().unwrap(), vec![goal]);
    }
    #[test]
    fn durable_records_survive_restart_and_duplicates_do_not_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("store.db");
        let p = ProjectInfo {
            id: "stable".into(),
            name: "Project".into(),
            root_path: "/project".into(),
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
