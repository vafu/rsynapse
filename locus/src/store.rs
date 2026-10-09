use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use locus::{RelationEndpoint, RelationRecord, RelationState};

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
struct StoredRecord {
    #[serde(flatten)]
    record: RelationRecord,
    // Only deserialization uses this default: old disk records were durable.
    #[serde(default = "legacy_persist")]
    persist: bool,
}

fn legacy_persist() -> bool {
    true
}

impl std::ops::Deref for StoredRecord {
    type Target = RelationRecord;
    fn deref(&self) -> &Self::Target {
        &self.record
    }
}

impl std::ops::DerefMut for StoredRecord {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.record
    }
}

#[derive(Debug)]
pub struct RelationStore {
    path: PathBuf,
    records: Vec<StoredRecord>,
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
        let loaded_records = match fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).map_err(invalid_data)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error),
        };
        let records = persistent_records(&loaded_records);
        let store = Self { path, records };
        if store.records.len() != loaded_records.len() {
            store.persist_records(&store.records)?;
        }
        Ok(store)
    }

    pub fn set(
        &mut self,
        subject: RelationEndpoint,
        relation: String,
        target: RelationEndpoint,
        metadata: HashMap<String, String>,
    ) -> io::Result<SetOutcome> {
        self.set_with_persistence(subject, relation, target, metadata, None)
    }

    pub fn set_with_persistence(
        &mut self,
        subject: RelationEndpoint,
        relation: String,
        target: RelationEndpoint,
        metadata: HashMap<String, String>,
        persist: Option<bool>,
    ) -> io::Result<SetOutcome> {
        validate_endpoint("subject", &subject)?;
        validate_relation(&relation)?;
        validate_endpoint("target", &target)?;

        let mut next = self.records.clone();
        let outcome = set_in_records(&mut next, subject, relation, target, metadata, persist);
        self.persist_changed_records(&next)?;
        self.records = next;
        Ok(outcome)
    }

    pub fn set_one(
        &mut self,
        subject: RelationEndpoint,
        relation: String,
        target: RelationEndpoint,
        metadata: HashMap<String, String>,
    ) -> io::Result<ReplaceOutcome> {
        self.set_one_with_persistence(subject, relation, target, metadata, None)
    }

    pub fn set_one_with_persistence(
        &mut self,
        subject: RelationEndpoint,
        relation: String,
        target: RelationEndpoint,
        metadata: HashMap<String, String>,
        persist: Option<bool>,
    ) -> io::Result<ReplaceOutcome> {
        validate_endpoint("subject", &subject)?;
        validate_relation(&relation)?;
        validate_endpoint("target", &target)?;

        let mut removed = Vec::new();
        let mut next = Vec::with_capacity(self.records.len());
        for record in &self.records {
            let should_remove =
                record.subject == subject && record.relation == relation && record.target != target;
            if should_remove {
                removed.push(record.record.clone());
            } else {
                next.push(record.clone());
            }
        }

        let set = set_in_records(&mut next, subject, relation, target, metadata, persist);
        self.persist_changed_records(&next)?;
        self.records = next;
        Ok(ReplaceOutcome { set, removed })
    }

    pub fn unset(
        &mut self,
        subject: &RelationEndpoint,
        relation: &str,
        target: &RelationEndpoint,
    ) -> io::Result<Option<RelationRecord>> {
        validate_endpoint("subject", subject)?;
        validate_relation(relation)?;
        validate_endpoint("target", target)?;

        let Some(index) = self.records.iter().position(|record| {
            &record.subject == subject && record.relation == relation && &record.target == target
        }) else {
            return Ok(None);
        };

        let mut next = self.records.clone();
        let record = next.remove(index);
        self.persist_changed_records(&next)?;
        self.records = next;
        Ok(Some(record.record))
    }

    pub fn clear(
        &mut self,
        subject: &RelationEndpoint,
        relation: &str,
    ) -> io::Result<Vec<RelationRecord>> {
        validate_endpoint("subject", subject)?;
        validate_relation(relation)?;

        let mut removed = Vec::new();
        let mut retained = Vec::with_capacity(self.records.len());
        for record in &self.records {
            if &record.subject == subject && record.relation == relation {
                removed.push(record.record.clone());
            } else {
                retained.push(record.clone());
            }
        }
        if !removed.is_empty() {
            self.persist_changed_records(&retained)?;
            self.records = retained;
        }
        Ok(removed)
    }

    pub fn clear_subject(&mut self, subject: &RelationEndpoint) -> io::Result<Vec<RelationRecord>> {
        validate_endpoint("subject", subject)?;

        let mut removed = Vec::new();
        let mut retained = Vec::with_capacity(self.records.len());
        for record in &self.records {
            if &record.subject == subject {
                removed.push(record.record.clone());
            } else {
                retained.push(record.clone());
            }
        }
        if !removed.is_empty() {
            self.persist_changed_records(&retained)?;
            self.records = retained;
        }
        Ok(removed)
    }

    pub fn targets(&self, subject: &RelationEndpoint, relation: &str) -> Vec<RelationEndpoint> {
        let mut targets = self
            .records
            .iter()
            .filter(|record| &record.subject == subject && record.relation == relation)
            .map(|record| record.target.clone())
            .collect::<Vec<_>>();
        targets.sort();
        targets
    }

    pub fn subjects(&self, relation: &str, target: &RelationEndpoint) -> Vec<RelationEndpoint> {
        let mut subjects = self
            .records
            .iter()
            .filter(|record| record.relation == relation && &record.target == target)
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
            .map(|record| record.record.clone())
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

    pub fn list_with_persistence(&self, relation: &str) -> Vec<RelationState> {
        let mut states = self
            .records
            .iter()
            .filter(|stored| relation.is_empty() || stored.relation == relation)
            .map(|stored| RelationState {
                record: stored.record.clone(),
                persist: stored.persist,
            })
            .collect::<Vec<_>>();
        states.sort_by(|left, right| {
            left.record
                .relation
                .cmp(&right.record.relation)
                .then_with(|| left.record.subject.cmp(&right.record.subject))
                .then_with(|| left.record.target.cmp(&right.record.target))
        });
        states
    }

    pub fn set_persistence(
        &mut self,
        subject: &RelationEndpoint,
        relation: &str,
        target: &RelationEndpoint,
        persist: bool,
    ) -> io::Result<RelationState> {
        validate_endpoint("subject", subject)?;
        validate_relation(relation)?;
        validate_endpoint("target", target)?;
        let mut next = self.records.clone();
        let record = next
            .iter_mut()
            .find(|record| {
                &record.subject == subject
                    && record.relation == relation
                    && &record.target == target
            })
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "relation record not found"))?;
        record.persist = persist;
        let state = RelationState {
            record: record.record.clone(),
            persist,
        };
        self.persist_changed_records(&next)?;
        self.records = next;
        Ok(state)
    }

    fn persist_changed_records(&self, records: &[StoredRecord]) -> io::Result<()> {
        if persistent_records(&self.records) == persistent_records(records) {
            return Ok(());
        }
        self.persist_records(records)
    }

    fn persist_records(&self, records: &[StoredRecord]) -> io::Result<()> {
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;

        let tmp = self.path.with_extension("json.tmp");
        let persistent = persistent_records(records);
        let data = serde_json::to_vec_pretty(&persistent).map_err(io::Error::other)?;
        use std::io::Write;
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&data)?;
        file.sync_all()?;
        fs::rename(tmp, &self.path)?;
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    }
}

fn persistent_records(records: &[StoredRecord]) -> Vec<StoredRecord> {
    records
        .iter()
        .filter(|record| record.persist)
        .cloned()
        .collect()
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

fn validate_relation(value: &str) -> io::Result<()> {
    if value.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "relation must not be empty",
        ));
    }
    Ok(())
}

fn validate_endpoint(name: &str, endpoint: &RelationEndpoint) -> io::Result<()> {
    match endpoint {
        RelationEndpoint::StableKey { kind, id } => {
            validate_nonblank(name, "kind", kind)?;
            validate_nonblank(name, "id", id)?;
        }
        RelationEndpoint::DBusObject {
            bus,
            service,
            path,
            interface,
        } => {
            validate_nonblank(name, "bus", bus)?;
            validate_nonblank(name, "service", service)?;
            validate_nonblank(name, "path", path)?;
            validate_nonblank(name, "interface", interface)?;
            zvariant::ObjectPath::try_from(path.as_str()).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("{name}.path must be a valid D-Bus object path: {error}"),
                )
            })?;
        }
    }
    Ok(())
}

fn validate_nonblank(endpoint: &str, field: &str, value: &str) -> io::Result<()> {
    if value.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{endpoint}.{field} must not be empty"),
        ));
    }
    Ok(())
}

fn set_in_records(
    records: &mut Vec<StoredRecord>,
    subject: RelationEndpoint,
    relation: String,
    target: RelationEndpoint,
    metadata: HashMap<String, String>,
    persist: Option<bool>,
) -> SetOutcome {
    let now = unix_ms();
    match records.iter_mut().find(|record| {
        record.subject == subject && record.relation == relation && record.target == target
    }) {
        Some(record) => {
            if let Some(persist) = persist {
                record.persist = persist;
            }
            record.metadata = metadata;
            record.updated_at_unix_ms = now;
            SetOutcome {
                record: record.record.clone(),
                created: false,
            }
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
            records.push(StoredRecord {
                record: record.clone(),
                persist: persist.unwrap_or(false),
            });
            SetOutcome {
                record,
                created: true,
            }
        }
    }
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
    use locus::keys;

    fn key(kind: &str, id: &str) -> RelationEndpoint {
        RelationEndpoint::stable_key(kind, id)
    }

    #[test]
    fn false_disk_entries_are_removed_and_failed_disabling_retains_policy() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("relations.json");
        let durable = StoredRecord {
            record: record(window(1), "test", project("one")),
            persist: true,
        };
        let transient = StoredRecord {
            record: record(workspace(2), "test", project("two")),
            persist: false,
        };
        fs::write(
            &path,
            serde_json::to_vec(&vec![durable.clone(), transient]).unwrap(),
        )
        .unwrap();
        let mut store = RelationStore::open(path.clone()).unwrap();
        assert_eq!(store.len(), 1);
        assert_eq!(
            serde_json::from_slice::<Vec<StoredRecord>>(&fs::read(&path).unwrap()).unwrap(),
            vec![durable]
        );
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        store
            .set_persistence(&window(1), "test", &project("one"), false)
            .unwrap_err();
        assert!(store.list_with_persistence("")[0].persist);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn defaults_toggles_snapshots_and_restart_are_endpoint_independent() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("relations.json");
        let mut store = RelationStore::open(path.clone()).unwrap();
        // Neither stable workspace IDs nor window IDs imply a persistence policy.
        for subject in [workspace(5), window(24)] {
            store
                .set(subject, "test".into(), project("test"), HashMap::new())
                .unwrap();
        }
        assert!(
            store
                .list_with_persistence("")
                .iter()
                .all(|state| !state.persist)
        );
        assert!(!path.exists());
        assert!(
            RelationStore::open(path.clone())
                .unwrap()
                .list("")
                .is_empty()
        );

        let before = store.list("");
        let enabled = store
            .set_persistence(&window(24), "test", &project("test"), true)
            .unwrap();
        assert!(enabled.persist);
        assert_eq!(before, store.list("")); // toggling changes neither metadata nor timestamps
        let restarted = RelationStore::open(path.clone()).unwrap();
        assert_eq!(restarted.list_with_persistence(""), vec![enabled.clone()]);
        let snapshot: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(snapshot[0]["persist"], true);

        // Legacy updates preserve explicit policy.
        store
            .set(window(24), "test".into(), project("test"), HashMap::new())
            .unwrap();
        assert!(
            store
                .list_with_persistence("")
                .iter()
                .find(|s| s.record.subject == window(24))
                .unwrap()
                .persist
        );
        store
            .set_persistence(&window(24), "test", &project("test"), false)
            .unwrap();
        assert_eq!(store.len(), 2);
        assert_eq!(fs::read_to_string(&path).unwrap(), "[]");
        assert_eq!(RelationStore::open(path).unwrap().len(), 0);
    }

    #[test]
    fn explicit_set_and_replace_commit_policy_atomically() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("relations.json");
        let mut store = RelationStore::open(path.clone()).unwrap();
        store
            .set_with_persistence(
                window(1),
                "test".into(),
                project("old"),
                HashMap::new(),
                Some(true),
            )
            .unwrap();
        store
            .set_one_with_persistence(
                window(1),
                "test".into(),
                project("new"),
                HashMap::new(),
                Some(false),
            )
            .unwrap();
        assert_eq!(store.targets(&window(1), "test"), vec![project("new")]);
        assert_eq!(RelationStore::open(path.clone()).unwrap().len(), 0);
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        store
            .set_persistence(&window(1), "test", &project("new"), true)
            .unwrap_err();
        assert!(!store.list_with_persistence("")[0].persist);
        store
            .set_persistence(&window(99), "test", &project("new"), true)
            .unwrap_err();
    }

    fn workspace(id: u64) -> RelationEndpoint {
        key("org.rsynapse.niri.workspace.id", &id.to_string())
    }

    fn window(id: u64) -> RelationEndpoint {
        key("org.rsynapse.niri.window.id", &id.to_string())
    }

    fn project(path: &str) -> RelationEndpoint {
        key("org.rsynapse.project.path", path)
    }

    fn agent(id: &str) -> RelationEndpoint {
        key("org.rsynapse.agent.session.id", id)
    }

    fn icon(glyph: &str) -> RelationEndpoint {
        key("org.rsynapse.icon.glyph", glyph)
    }

    fn record(
        subject: RelationEndpoint,
        relation: &str,
        target: RelationEndpoint,
    ) -> RelationRecord {
        RelationRecord {
            subject,
            relation: relation.to_owned(),
            target,
            metadata: HashMap::new(),
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
        }
    }

    #[test]
    fn set_query_unset_and_reload() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("relations.json");
        let mut store = RelationStore::open(path.clone()).expect("open store");
        let outcome = store
            .set(
                workspace(5),
                "org.rsynapse.WorkspaceProject".to_owned(),
                project("rsynapse"),
                HashMap::from([("source".to_owned(), "test".to_owned())]),
            )
            .expect("set");
        let record = outcome.record;

        assert!(outcome.created);
        store
            .set_persistence(&record.subject, &record.relation, &record.target, true)
            .expect("enable persistence");
        assert_eq!(record.created_at_unix_ms, record.updated_at_unix_ms);
        assert_eq!(
            store.targets(&workspace(5), "org.rsynapse.WorkspaceProject"),
            vec![project("rsynapse")]
        );

        let store = RelationStore::open(path.clone()).expect("reload store");
        assert_eq!(store.list("org.rsynapse.WorkspaceProject").len(), 1);

        let mut store = RelationStore::open(path).expect("reload mutable store");
        assert!(
            store
                .unset(
                    &workspace(5),
                    "org.rsynapse.WorkspaceProject",
                    &project("rsynapse"),
                )
                .expect("unset")
                .is_some()
        );
        assert!(
            store
                .targets(&workspace(5), "org.rsynapse.WorkspaceProject")
                .is_empty()
        );
    }

    #[test]
    fn clear_subject_removes_only_subject_owned_relations() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("relations.json");
        let mut store = RelationStore::open(path).expect("open store");
        let workspace = workspace(7);
        let project = project("coroutines");

        store
            .set(
                workspace.clone(),
                "org.rsynapse.workspace.project".to_owned(),
                project.clone(),
                HashMap::new(),
            )
            .expect("set workspace project");
        store
            .set(
                workspace.clone(),
                "org.rsynapse.workspace.icon-override".to_owned(),
                icon("code"),
                HashMap::new(),
            )
            .expect("set workspace icon");
        store
            .set(
                project,
                "org.rsynapse.project.metadata".to_owned(),
                key("org.rsynapse.project.path", "coroutines"),
                HashMap::new(),
            )
            .expect("set project metadata");

        assert_eq!(
            store
                .clear_subject(&workspace)
                .expect("clear subject")
                .len(),
            2
        );
        assert!(store.list("org.rsynapse.workspace.project").is_empty());
        assert_eq!(store.list("org.rsynapse.project.metadata").len(), 1);
    }

    #[test]
    fn named_workspace_icon_override_survives_reload() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("relations.json");
        let subject = key(keys::NIRI_WORKSPACE_NAME, "coding");
        let relation = "org.rsynapse.workspace.icon-override";
        let target = icon("glyph");
        let mut store = RelationStore::open(path.clone()).expect("open store");

        store
            .set_one(
                subject.clone(),
                relation.to_owned(),
                target.clone(),
                HashMap::from([("pick-icon-input".to_owned(), "rust".to_owned())]),
            )
            .expect("set icon override");

        store
            .set_persistence(&subject, relation, &target, true)
            .expect("persist icon override");

        let store = RelationStore::open(path).expect("reload store");
        assert_eq!(store.targets(&subject, relation), vec![target]);
    }

    #[test]
    fn set_updates_existing_record_without_duplicating_it() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("relations.json");
        let mut store = RelationStore::open(path).expect("open store");
        let first = store
            .set(
                window(1),
                "org.rsynapse.WindowAgent".to_owned(),
                agent("codex"),
                HashMap::from([("state".to_owned(), "thinking".to_owned())]),
            )
            .expect("first set");
        let second = store
            .set(
                window(1),
                "org.rsynapse.WindowAgent".to_owned(),
                agent("codex"),
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
    fn new_window_relations_default_to_memory_only() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("relations.json");
        let mut store = RelationStore::open(path.clone()).expect("open store");
        store
            .set(
                window(24),
                "org.rsynapse.window.agent-session".to_owned(),
                agent("codex/session"),
                HashMap::new(),
            )
            .expect("set transient window relation");

        assert_eq!(
            store.targets(&window(24), "org.rsynapse.window.agent-session"),
            vec![agent("codex/session")]
        );

        let store = RelationStore::open(path).expect("reload store");
        assert!(
            store
                .targets(&window(24), "org.rsynapse.window.agent-session")
                .is_empty()
        );
    }

    #[test]
    fn migration_preserves_all_legacy_disk_records_as_persistent() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("relations.json");
        let records = vec![
            record(
                window(24),
                "org.rsynapse.window.agent-session",
                agent("codex/session"),
            ),
            record(
                workspace(3),
                "org.rsynapse.workspace.project",
                project("rsynapse"),
            ),
        ];
        fs::write(
            &path,
            serde_json::to_vec(&records).expect("serialize records"),
        )
        .expect("write records");

        let store = RelationStore::open(path.clone()).expect("open store");

        assert_eq!(store.list("org.rsynapse.window.agent-session").len(), 1);
        assert_eq!(store.list("org.rsynapse.workspace.project").len(), 1);
        assert!(
            store
                .list_with_persistence("")
                .iter()
                .all(|state| state.persist)
        );

        let persisted: Vec<RelationRecord> =
            serde_json::from_slice(&fs::read(path).expect("read cleaned persistent store"))
                .expect("parse cleaned persistent store");
        assert_eq!(persisted.len(), 2);
    }

    #[test]
    fn transient_window_relations_do_not_require_persistence() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("relations.json");
        let mut store = RelationStore::open(path.clone()).expect("open store");
        fs::create_dir(&path).expect("replace store path with directory");

        store
            .set(
                window(24),
                "org.rsynapse.window.agent-session".to_owned(),
                agent("codex/session"),
                HashMap::new(),
            )
            .expect("set transient window relation");

        assert_eq!(
            store.targets(&window(24), "org.rsynapse.window.agent-session"),
            vec![agent("codex/session")]
        );
        assert!(
            store
                .unset(
                    &window(24),
                    "org.rsynapse.window.agent-session",
                    &agent("codex/session"),
                )
                .expect("unset transient window relation")
                .is_some()
        );
        store
            .set(
                window(24),
                "org.rsynapse.window.agent-session".to_owned(),
                agent("codex/session"),
                HashMap::new(),
            )
            .expect("set transient window relation again");
        assert_eq!(
            store
                .clear(&window(24), "org.rsynapse.window.agent-session")
                .expect("clear transient window relation")
                .len(),
            1
        );
    }

    #[test]
    fn set_one_replaces_other_targets_for_subject_relation() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("relations.json");
        let mut store = RelationStore::open(path).expect("open store");
        store
            .set(
                workspace(1),
                "org.rsynapse.WorkspaceProject".to_owned(),
                project("old"),
                HashMap::new(),
            )
            .expect("set old");

        let outcome = store
            .set_one(
                workspace(1),
                "org.rsynapse.WorkspaceProject".to_owned(),
                project("new"),
                HashMap::new(),
            )
            .expect("replace");

        assert_eq!(outcome.removed.len(), 1);
        assert_eq!(
            store.targets(&workspace(1), "org.rsynapse.WorkspaceProject"),
            vec![project("new")]
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
                    key("subject", relation),
                    relation.to_owned(),
                    key("target", "one"),
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
                key(" ", "subject"),
                "org.rsynapse.WorkspaceProject".to_owned(),
                project("rsynapse"),
                HashMap::new(),
            )
            .expect_err("blank subject rejected");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn failed_set_persistence_does_not_change_memory() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("relations.json");
        let mut store = RelationStore::open(path.clone()).expect("open store");
        store
            .set(
                workspace(1),
                "org.rsynapse.WorkspaceProject".to_owned(),
                project("old"),
                HashMap::new(),
            )
            .expect("initial set");

        store
            .set_persistence(
                &workspace(1),
                "org.rsynapse.WorkspaceProject",
                &project("old"),
                true,
            )
            .expect("persist initial record");

        fs::remove_file(&path).expect("remove persisted file");
        fs::create_dir(&path).expect("replace store path with directory");

        let error = store
            .set_with_persistence(
                workspace(1),
                "org.rsynapse.WorkspaceProject".to_owned(),
                project("new"),
                HashMap::new(),
                Some(true),
            )
            .expect_err("persist should fail");
        assert_ne!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(
            store.targets(&workspace(1), "org.rsynapse.WorkspaceProject"),
            vec![project("old")]
        );
    }

    #[test]
    fn failed_clear_persistence_does_not_change_memory() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("relations.json");
        let mut store = RelationStore::open(path.clone()).expect("open store");
        store
            .set(
                workspace(1),
                "org.rsynapse.WorkspaceProject".to_owned(),
                project("old"),
                HashMap::new(),
            )
            .expect("initial set");

        store
            .set_persistence(
                &workspace(1),
                "org.rsynapse.WorkspaceProject",
                &project("old"),
                true,
            )
            .expect("persist initial record");

        fs::remove_file(&path).expect("remove persisted file");
        fs::create_dir(&path).expect("replace store path with directory");

        store
            .clear(&workspace(1), "org.rsynapse.WorkspaceProject")
            .expect_err("persist should fail");
        assert_eq!(
            store.targets(&workspace(1), "org.rsynapse.WorkspaceProject"),
            vec![project("old")]
        );
    }
}
