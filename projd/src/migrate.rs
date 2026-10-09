use proj_model::{CheckoutInfo, ProjectInfo};
use rusqlite::{Connection, params};
use serde::Deserialize;
use std::{collections::HashSet, path::Path};
#[derive(Deserialize)]
struct LegacyProject {
    id: String,
    name: String,
    root_path: String,
}
#[derive(Deserialize)]
struct LegacyCheckout {
    id: String,
    project_id: String,
    root_path: String,
    branch: String,
    git_status: proj_model::GitStatus,
}
pub(super) fn run(db: &mut Connection) -> anyhow::Result<()> {
    let projects: Vec<serde_json::Value> = {
        let mut s = db.prepare("SELECT json FROM records WHERE kind='project'")?;
        let rows = s.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?))
            .collect::<anyhow::Result<_>>()?
    };
    if db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))? >= 2 {
        return Ok(());
    }
    let checkouts: Vec<LegacyCheckout> = {
        let mut s = db.prepare("SELECT json FROM records WHERE kind='checkout' AND json_extract(json,'$.project_id') IS NOT NULL ORDER BY id")?;
        let rows = s.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?))
            .collect::<anyhow::Result<_>>()?
    };
    let tx = db.transaction()?;
    for value in projects {
        if value.get("cwd").is_some() {
            continue;
        }
        let old: LegacyProject = serde_json::from_value(value)?;
        let owned: Vec<_> = checkouts
            .iter()
            .filter(|c| c.project_id == old.id)
            .collect();
        let preferred = owned
            .iter()
            .find(|c| c.root_path == old.root_path)
            .or_else(|| owned.first())
            .map(|c| c.id.clone());
        let mut used = HashSet::new();
        tx.execute(
            "DELETE FROM records WHERE kind='project' AND id=?",
            [&old.id],
        )?;
        for checkout in owned {
            let root = Path::new(&checkout.root_path);
            // An unavailable checkout cannot be classified as a non-Git directory.
            // Retain its identity until the filesystem is available again.
            let git = !checkout.branch.is_empty() || root.join(".git").exists() || !root.is_dir();
            let keep_id = preferred.as_deref() == Some(&checkout.id);
            let project = ProjectInfo {
                id: if keep_id {
                    old.id.clone()
                } else {
                    uuid::Uuid::new_v4().simple().to_string()
                },
                name: if keep_id
                    && old.name
                        != Path::new(&old.root_path)
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                {
                    old.name.clone()
                } else {
                    Path::new(&checkout.root_path)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or(&old.name)
                        .to_owned()
                },
                cwd: checkout.root_path.clone(),
                checkout_id: if git {
                    checkout.id.clone()
                } else {
                    String::new()
                },
                icon: String::new(),
                icon_origin: String::new(),
            };
            tx.execute(
                "INSERT INTO records(kind,id,json) VALUES('project',?,?)",
                params![project.id, serde_json::to_string(&project)?],
            )?;
            if git {
                let c = CheckoutInfo {
                    id: checkout.id.clone(),
                    root_path: checkout.root_path.clone(),
                    branch: checkout.branch.clone(),
                    git_status: checkout.git_status.clone(),
                };
                tx.execute(
                    "UPDATE records SET json=? WHERE kind='checkout' AND id=?",
                    params![serde_json::to_string(&c)?, c.id],
                )?;
            } else {
                tx.execute(
                    "DELETE FROM records WHERE kind='checkout' AND id=?",
                    [&checkout.id],
                )?;
            }
            used.insert(project.id);
        }
        if used.is_empty() {
            let p = ProjectInfo {
                id: old.id,
                name: old.name,
                cwd: old.root_path,
                checkout_id: String::new(),
                icon: String::new(),
                icon_origin: String::new(),
            };
            tx.execute(
                "INSERT INTO records(kind,id,json) VALUES('project',?,?)",
                params![p.id, serde_json::to_string(&p)?],
            )?;
        }
    }
    tx.execute("DELETE FROM records WHERE kind='context'", [])?;
    tx.execute_batch("PRAGMA user_version=2;")?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_only_legacy_store_is_cleaned_and_versioned() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE records(kind TEXT,id TEXT,json TEXT,PRIMARY KEY(kind,id)); INSERT INTO records VALUES('context','orphan','{}'); INSERT INTO records VALUES('goal','history','{\"title\":\"Preserve\"}');").unwrap();
        run(&mut db).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM records WHERE kind='context'",
                [],
                |r| r.get::<_, u32>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(
            db.query_row("SELECT json FROM records WHERE kind='goal'", [], |r| r
                .get::<_, String>(
                0
            ))
            .unwrap(),
            "{\"title\":\"Preserve\"}"
        );
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
                .unwrap(),
            2
        );
        run(&mut db).unwrap();
    }

    #[test]
    fn unavailable_branchless_checkout_keeps_its_identity() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp
            .path()
            .join("unavailable")
            .to_string_lossy()
            .into_owned();
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE records(kind TEXT,id TEXT,json TEXT,PRIMARY KEY(kind,id));")
            .unwrap();
        let project = serde_json::json!({"id":"p", "name":"Custom", "root_path":root});
        let checkout = serde_json::json!({"id":"c", "project_id":"p", "root_path":root, "branch":"", "git_status":proj_model::GitStatus::default()});
        for (kind, id, value) in [("project", "p", project), ("checkout", "c", checkout)] {
            db.execute(
                "INSERT INTO records VALUES(?,?,?)",
                params![kind, id, value.to_string()],
            )
            .unwrap();
        }
        run(&mut db).unwrap();
        let project: ProjectInfo = serde_json::from_str(
            &db.query_row("SELECT json FROM records WHERE kind='project'", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            (project.id.as_str(), project.checkout_id.as_str()),
            ("p", "c")
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM records WHERE kind='checkout'",
                [],
                |r| r.get::<_, u32>(0)
            )
            .unwrap(),
            1
        );
    }
}
