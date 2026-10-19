//! Durable descriptions only. Live identities and cross-service links stay out
//! of this file; all IPC is D-Bus.
use super::model::Session;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
};

#[derive(Clone, Default, Serialize, Deserialize)]
struct Data {
    sessions: BTreeMap<String, Session>,
}

pub struct Store {
    path: PathBuf,
    data: Data,
}
impl Store {
    pub fn open(path: PathBuf) -> anyhow::Result<Self> {
        let data: Data = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Data::default(),
            Err(e) => return Err(e.into()),
        };
        for session in data.sessions.values() {
            session.validate()?;
        }
        Ok(Self { path, data })
    }
    pub fn get(&self, name: &str) -> anyhow::Result<Session> {
        self.data
            .sessions
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no saved session {name:?}"))
    }
    pub fn list(&self) -> Vec<String> {
        self.data.sessions.keys().cloned().collect()
    }
    pub fn put(&mut self, session: Session) -> anyhow::Result<()> {
        session.validate()?;
        if self.data.sessions.get(&session.name) == Some(&session) {
            return Ok(());
        }
        let mut next = self.data.clone();
        // Retain the previous complete automatic checkpoint as well as any
        // explicitly named saves. A disconnect never writes an empty session.
        if session.name == "last" {
            if let Some(mut previous) = next.sessions.get("last").cloned() {
                previous.name = "previous".into();
                next.sessions.insert("previous".into(), previous);
            }
        }
        next.sessions.insert(session.name.clone(), session);
        let parent = self
            .path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("session store has no parent"))?;
        fs::create_dir_all(parent)?;
        let temporary = self
            .path
            .with_extension(format!("{}.tmp", super::model::id()));
        let result = (|| -> anyhow::Result<()> {
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&temporary)?;
            file.write_all(&serde_json::to_vec_pretty(&next)?)?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)?;
            File::open(parent)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
        self.data = next;
        Ok(())
    }
}

pub fn path() -> PathBuf {
    std::env::var_os("STEWARD_SESSION_STORE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("XDG_STATE_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(std::env::var_os("HOME").unwrap()).join(".local/state")
                })
                .join("rsynapse/steward/sessions.json")
        })
}
