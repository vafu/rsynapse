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
            relations::records(self.locus.clone(), relations::WORKSPACE_NAME),
            shell_source::proj::projects()
            => |(focused, projects, names, metadata)| (focused, projects, names, metadata),
        )
        .distinct_until_changed()
        .box_it()
        .into_stream();
        while let Some(item) = states.next().await {
            match item {
                Ok((Some(id), projects, names, metadata)) => {
                    let subject = workspace_subject(id);
                    let current = names.iter().find(|record| record.subject == subject);
                    let project = projects
                        .iter()
                        .find(|r| r.subject == subject)
                        .and_then(|r| r.metadata.get("project-id"))
                        .and_then(|id| metadata.iter().find(|p| &p.id == id))
                        .map(|p| p.name.clone());
                    if let Some((name, source)) = name_update(current, project) {
                        let target =
                            RelationEndpoint::stable_key(relations::WORKSPACE_NAME_KIND, name);
                        let metadata = HashMap::from([("source".to_owned(), source.to_owned())]);
                        self.locus
                            .set_one_with_persistence(
                                subject,
                                relations::WORKSPACE_NAME,
                                target,
                                metadata,
                                false,
                            )
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

/// Compare before writing: manual names survive, automatic names follow Project.Name,
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
    fn auto_names_follow_stored_project_name() {
        assert_eq!(
            name_update(Some(&record("empty", "default")), Some("cwd".to_owned())),
            Some(("cwd".to_owned(), "project"))
        );
    }
}
