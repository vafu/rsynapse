use proj_model::{CheckoutInfo, ProjectInfo, RemovedProjectInfo};
use rusqlite::{Connection, params};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};
mod labels;
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
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS records(kind TEXT NOT NULL,id TEXT NOT NULL,json TEXT NOT NULL,PRIMARY KEY(kind,id));")?;
        let legacy:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM records WHERE (kind='project' AND json_extract(json,'$.cwd') IS NULL) OR kind='context')",[],|r|r.get(0))?;
        if legacy {
            let backup = path.with_file_name(format!(
                "before-model-v2-{}.sqlite3",
                chrono::Utc::now().timestamp_micros()
            ));
            db.execute("VACUUM INTO ?", [backup.to_string_lossy().as_ref()])?;
        }
        crate::migrate::run(&mut db)?;
        Ok(Self { db: Mutex::new(db) })
    }
    pub fn list<T: DeserializeOwned>(&self, kind: &str) -> anyhow::Result<Vec<T>> {
        read(&self.db.lock().unwrap(), kind)
    }
    pub fn put<T: Serialize>(&self, kind: &str, id: &str, value: &T) -> anyhow::Result<()> {
        self.db.lock().unwrap().execute("INSERT INTO records(kind,id,json) VALUES(?,?,?) ON CONFLICT(kind,id) DO UPDATE SET json=excluded.json",params![kind,id,serde_json::to_string(value)?])?;
        Ok(())
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
        tx.commit()?;
        Ok(Some(removed))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
