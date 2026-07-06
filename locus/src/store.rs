use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use zvariant::Type;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
pub struct RelationRecord {
    pub subject: String,
    pub relation: String,
    pub target: String,
    pub metadata: HashMap<String, String>,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
}

#[derive(Debug)]
pub struct RelationStore {
    path: PathBuf,
    records: Vec<RelationRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetOutcome {
    pub record: RelationRecord,
    pub created: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplaceOutcome {
    pub set: SetOutcome,
    pub removed: Vec<RelationRecord>,
}

impl RelationStore {
    pub fn open(path: PathBuf) -> io::Result<Self> {
        let records = match fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).map_err(invalid_data)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error),
        };
        Ok(Self { path, records })
    }

    pub fn set(
        &mut self,
        subject: String,
        relation: String,
        target: String,
        metadata: HashMap<String, String>,
    ) -> io::Result<SetOutcome> {
        validate_ref("subject", &subject)?;
        validate_ref("relation", &relation)?;
        validate_ref("target", &target)?;

        let now = unix_ms();
        let (record, created) = match self.records.iter_mut().find(|record| {
            record.subject == subject && record.relation == relation && record.target == target
        }) {
            Some(record) => {
                record.metadata = metadata;
                record.updated_at_unix_ms = now;
                (record.clone(), false)
            }
            None => {
                let record = RelationRecord {
                    subject,
                    relation,
                    target,
                    metadata,
                    created_at_unix_ms: now,
                    updated_at_unix_ms: now,
                };
                self.records.push(record.clone());
                (record, true)
            }
        };

        self.persist()?;
        Ok(SetOutcome { record, created })
    }

    pub fn set_one(
        &mut self,
        subject: String,
        relation: String,
        target: String,
        metadata: HashMap<String, String>,
    ) -> io::Result<ReplaceOutcome> {
        validate_ref("subject", &subject)?;
        validate_ref("relation", &relation)?;
        validate_ref("target", &target)?;

        let mut removed = Vec::new();
        self.records.retain(|record| {
            let should_remove =
                record.subject == subject && record.relation == relation && record.target != target;
            if should_remove {
                removed.push(record.clone());
            }
            !should_remove
        });

        let set = self.set(subject, relation, target, metadata)?;
        Ok(ReplaceOutcome { set, removed })
    }

    pub fn unset(
        &mut self,
        subject: &str,
        relation: &str,
        target: &str,
    ) -> io::Result<Option<RelationRecord>> {
        let Some(index) = self.records.iter().position(|record| {
            record.subject == subject && record.relation == relation && record.target == target
        }) else {
            return Ok(None);
        };

        let record = self.records.remove(index);
        self.persist()?;
        Ok(Some(record))
    }

    pub fn clear(&mut self, subject: &str, relation: &str) -> io::Result<Vec<RelationRecord>> {
        let mut removed = Vec::new();
        let mut retained = Vec::with_capacity(self.records.len());
        for record in self.records.drain(..) {
            if record.subject == subject && record.relation == relation {
                removed.push(record);
            } else {
                retained.push(record);
            }
        }
        self.records = retained;
        if !removed.is_empty() {
            self.persist()?;
        }
        Ok(removed)
    }

    pub fn targets(&self, subject: &str, relation: &str) -> Vec<String> {
        let mut targets = self
            .records
            .iter()
            .filter(|record| record.subject == subject && record.relation == relation)
            .map(|record| record.target.clone())
            .collect::<Vec<_>>();
        targets.sort();
        targets
    }

    pub fn subjects(&self, relation: &str, target: &str) -> Vec<String> {
        let mut subjects = self
            .records
            .iter()
            .filter(|record| record.relation == relation && record.target == target)
            .map(|record| record.subject.clone())
            .collect::<Vec<_>>();
        subjects.sort();
        subjects
    }

    pub fn list(&self, relation: &str) -> Vec<RelationRecord> {
        let mut records = self
            .records
            .iter()
            .filter(|record| relation.is_empty() || record.relation == relation)
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by(|left, right| {
            left.relation
                .cmp(&right.relation)
                .then_with(|| left.subject.cmp(&right.subject))
                .then_with(|| left.target.cmp(&right.target))
        });
        records
    }

    pub fn relations(&self) -> Vec<String> {
        let mut relations = self
            .records
            .iter()
            .map(|record| record.relation.clone())
            .collect::<Vec<_>>();
        relations.sort();
        relations.dedup();
        relations
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn persist(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp = self.path.with_extension("json.tmp");
        let data = serde_json::to_vec_pretty(&self.records).map_err(io::Error::other)?;
        fs::write(&tmp, data)?;
        fs::rename(tmp, &self.path)
    }
}

pub fn default_store_path() -> PathBuf {
    if let Some(path) = std::env::var_os("LOCUS_RELATIONS_PATH") {
        return PathBuf::from(path);
    }

    let state_home = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from("."));

    state_home.join("rsynapse/locus/relations.json")
}

fn validate_ref(name: &str, value: &str) -> io::Result<()> {
    if value.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name} must not be empty"),
        ));
    }
    Ok(())
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn invalid_data(error: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_query_unset_and_reload() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("relations.json");
        let mut store = RelationStore::open(path.clone()).expect("open store");
        let outcome = store
            .set(
                "niri-workspace:5".to_owned(),
                "org.rsynapse.WorkspaceProject".to_owned(),
                "project:rsynapse".to_owned(),
                HashMap::from([("source".to_owned(), "test".to_owned())]),
            )
            .expect("set");
        let record = outcome.record;

        assert!(outcome.created);
        assert_eq!(record.created_at_unix_ms, record.updated_at_unix_ms);
        assert_eq!(
            store.targets("niri-workspace:5", "org.rsynapse.WorkspaceProject"),
            vec!["project:rsynapse"]
        );

        let store = RelationStore::open(path.clone()).expect("reload store");
        assert_eq!(store.list("org.rsynapse.WorkspaceProject").len(), 1);

        let mut store = RelationStore::open(path).expect("reload mutable store");
        assert!(
            store
                .unset(
                    "niri-workspace:5",
                    "org.rsynapse.WorkspaceProject",
                    "project:rsynapse",
                )
                .expect("unset")
                .is_some()
        );
        assert!(
            store
                .targets("niri-workspace:5", "org.rsynapse.WorkspaceProject")
                .is_empty()
        );
    }

    #[test]
    fn set_updates_existing_record_without_duplicating_it() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("relations.json");
        let mut store = RelationStore::open(path).expect("open store");
        let first = store
            .set(
                "window:1".to_owned(),
                "org.rsynapse.WindowAgent".to_owned(),
                "agent:codex".to_owned(),
                HashMap::from([("state".to_owned(), "thinking".to_owned())]),
            )
            .expect("first set");
        let second = store
            .set(
                "window:1".to_owned(),
                "org.rsynapse.WindowAgent".to_owned(),
                "agent:codex".to_owned(),
                HashMap::from([("state".to_owned(), "idle".to_owned())]),
            )
            .expect("second set");

        assert!(first.created);
        assert!(!second.created);
        assert_eq!(store.list("").len(), 1);
        assert_eq!(
            second.record.metadata,
            HashMap::from([("state".to_owned(), "idle".to_owned())])
        );
        assert_eq!(
            first.record.created_at_unix_ms,
            second.record.created_at_unix_ms
        );
    }

    #[test]
    fn set_one_replaces_other_targets_for_subject_relation() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("relations.json");
        let mut store = RelationStore::open(path).expect("open store");
        store
            .set(
                "workspace:1".to_owned(),
                "org.rsynapse.WorkspaceProject".to_owned(),
                "project:old".to_owned(),
                HashMap::new(),
            )
            .expect("set old");

        let outcome = store
            .set_one(
                "workspace:1".to_owned(),
                "org.rsynapse.WorkspaceProject".to_owned(),
                "project:new".to_owned(),
                HashMap::new(),
            )
            .expect("replace");

        assert_eq!(outcome.removed.len(), 1);
        assert_eq!(
            store.targets("workspace:1", "org.rsynapse.WorkspaceProject"),
            vec!["project:new"]
        );
    }

    #[test]
    fn relations_are_sorted_and_deduplicated() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("relations.json");
        let mut store = RelationStore::open(path).expect("open store");
        for relation in [
            "org.rsynapse.WindowAgent",
            "org.rsynapse.WorkspaceProject",
            "org.rsynapse.WindowAgent",
        ] {
            store
                .set(
                    format!("subject:{relation}"),
                    relation.to_owned(),
                    "target:one".to_owned(),
                    HashMap::new(),
                )
                .expect("set relation");
        }

        assert_eq!(
            store.relations(),
            vec!["org.rsynapse.WindowAgent", "org.rsynapse.WorkspaceProject"]
        );
    }

    #[test]
    fn rejects_blank_references() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("relations.json");
        let mut store = RelationStore::open(path).expect("open store");
        let error = store
            .set(
                " ".to_owned(),
                "org.rsynapse.WorkspaceProject".to_owned(),
                "project:rsynapse".to_owned(),
                HashMap::new(),
            )
            .expect_err("blank subject rejected");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
}
