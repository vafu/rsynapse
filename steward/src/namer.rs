use crate::relations::{self, LocusClient};
use futures_util::StreamExt;
use locus::{RelationEndpoint, RelationRecord};
use shell_rx_macros::combine_latest;
use shell_source::{
    Observable,
    dbus::{self, Bus, ObjectDescriptor, PropertyDescriptor},
    rx::Observable as _,
};
use std::collections::HashMap;
use zbus::zvariant::OwnedObjectPath;

const WORKSPACE_PATH_PREFIX: &str = "/org/rsynapse/Niri/Workspaces/workspace_";

pub struct Namer {
    locus: LocusClient,
}

impl Namer {
    pub fn new(locus: LocusClient) -> Self {
        Self { locus }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let mut states = combine_latest!(
            focused_workspace_id(),
            relations::records(self.locus.clone(), relations::WORKSPACE_PROJECT),
            relations::records(self.locus.clone(), relations::WORKSPACE_NAME)
            => |(focused, projects, names)| (focused, projects, names),
        )
        .distinct_until_changed()
        .box_it()
        .into_stream();
        while let Some(item) = states.next().await {
            match item {
                Ok((Some(id), projects, names)) => {
                    let subject = workspace_subject(id);
                    let current = names.iter().find(|record| record.subject == subject);
                    let project = project_name_for(id, &projects);
                    if let Some((name, source)) = name_update(current, project) {
                        let target =
                            RelationEndpoint::stable_key(relations::WORKSPACE_NAME_KIND, name);
                        let metadata = HashMap::from([("source".to_owned(), source.to_owned())]);
                        self.locus
                            .set_one(subject, relations::WORKSPACE_NAME, target, metadata)
                            .await?;
                    }
                }
                Ok(_) => {}
                Err(error) => eprintln!("[rsynapse-steward/namer] failed: {error}"),
            }
        }
        Ok(())
    }
}

/// Compare before writing: manual names survive, automatic names follow cwd,
/// and old random defaults migrate once. Our own signals become cheap noops.
fn name_update(
    current: Option<&RelationRecord>,
    project: Option<String>,
) -> Option<(String, &'static str)> {
    if let Some(record) = current {
        let source = record.metadata.get("source").map(String::as_str);
        if !matches!(source, Some("project" | "random" | "default")) {
            return None;
        }
    }
    let (name, source) = match project {
        Some(name) => (name, "project"),
        None => ("empty".to_owned(), "default"),
    };
    if let Some(record) = current {
        if let RelationEndpoint::StableKey { kind, id } = &record.target {
            if kind == relations::WORKSPACE_NAME_KIND
                && id == &name
                && record.metadata.get("source").map(String::as_str) == Some(source)
            {
                return None;
            }
        }
    }
    Some((name, source))
}

fn project_name_for(id: u64, projects: &[RelationRecord]) -> Option<String> {
    let subject = workspace_subject(id);
    let record = projects.iter().find(|record| record.subject == subject)?;
    // Same cwd label policy as the shell's ProjectDetails: relative cwd,
    // then cwd basename, then project path basename. Not display-main.
    for key in ["relative-cwd", "cwd"] {
        if let Some(value) = record.metadata.get(key).map(|s| s.trim()) {
            if !value.is_empty() && value != "." {
                return Some(value.to_owned());
            }
        }
    }
    if let (Some(root), Some(cwd)) = (record.metadata.get("path"), record.metadata.get("cwd-path"))
    {
        if let Ok(relative) = std::path::Path::new(cwd).strip_prefix(root) {
            if !relative.as_os_str().is_empty() {
                return Some(relative.to_string_lossy().into_owned());
            }
        }
    }
    for key in ["cwd-path", "path"] {
        if let Some(name) = record
            .metadata
            .get(key)
            .and_then(|p| std::path::Path::new(p).file_name())
            .and_then(|s| s.to_str())
        {
            return Some(name.to_owned());
        }
    }
    if let RelationEndpoint::StableKey { kind, id } = &record.target {
        if kind == locus::keys::PROJECT_PATH {
            return std::path::Path::new(id)
                .file_name()
                .and_then(|s| s.to_str())
                .map(str::to_owned);
        }
    }
    None
}

fn workspace_subject(id: u64) -> RelationEndpoint {
    RelationEndpoint::stable_key(locus::keys::NIRI_WORKSPACE_ID, id.to_string())
}

fn focused_workspace_id() -> Observable<Option<u64>> {
    let descriptor = ObjectDescriptor::parse(
        Bus::Session,
        niri_dbus::BUS_NAME,
        niri_dbus::ROOT_PATH,
        niri_dbus::ROOT_INTERFACE,
    )
    .expect("niri descriptor");
    dbus::optional_array_property::<OwnedObjectPath>(PropertyDescriptor::new(
        descriptor,
        "FocusedWorkspace",
    ))
    .map(|path| {
        path.and_then(|path| {
            path.as_str()
                .strip_prefix(WORKSPACE_PATH_PREFIX)?
                .parse()
                .ok()
        })
    })
    .distinct_until_changed()
    .box_it()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(name: &str, source: &str) -> RelationRecord {
        RelationRecord {
            subject: workspace_subject(7),
            relation: relations::WORKSPACE_NAME.to_owned(),
            target: RelationEndpoint::stable_key(relations::WORKSPACE_NAME_KIND, name),
            metadata: HashMap::from([("source".to_owned(), source.to_owned())]),
            created_at_unix_ms: 0,
            updated_at_unix_ms: 0,
        }
    }
    #[test]
    fn defaults_are_empty_and_idempotent() {
        assert_eq!(
            name_update(None, None),
            Some(("empty".to_owned(), "default"))
        );
        assert_eq!(name_update(Some(&record("empty", "default")), None), None);
        assert_eq!(
            name_update(Some(&record("noble-owl", "random")), None),
            Some(("empty".to_owned(), "default"))
        );
    }
    #[test]
    fn manual_names_survive_project_updates() {
        assert_eq!(
            name_update(
                Some(&record("my workspace", "manual")),
                Some("cwd".to_owned())
            ),
            None
        );
    }
    #[test]
    fn auto_names_follow_project_cwd() {
        assert_eq!(
            name_update(Some(&record("empty", "default")), Some("cwd".to_owned())),
            Some(("cwd".to_owned(), "project"))
        );
        let mut project = record("ignored", "project");
        project.metadata = HashMap::from([
            ("path".to_owned(), "/repo".to_owned()),
            ("cwd-path".to_owned(), "/repo/subdir".to_owned()),
            ("display-main".to_owned(), "wrong".to_owned()),
        ]);
        assert_eq!(project_name_for(7, &[project]), Some("subdir".to_owned()));
    }
}
