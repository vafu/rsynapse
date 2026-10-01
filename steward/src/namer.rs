use std::collections::{HashMap, HashSet};

use futures_util::StreamExt;
use locus::{RelationEndpoint, RelationRecord};
use shell_rx_macros::combine_latest;
use shell_source::{
    Observable,
    dbus::{self, Bus, ObjectDescriptor, PropertyDescriptor},
    rx::Observable as _,
};
use zbus::zvariant::OwnedObjectPath;

use crate::relations::{self, LocusClient};

const WORKSPACE_PATH_PREFIX: &str = "/org/rsynapse/Niri/Workspaces/workspace_";

const SOURCE_PROJECT: &str = "project";
const SOURCE_RANDOM: &str = "random";

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
                Ok((focused, projects, names)) => {
                    self.maybe_name(focused, &projects, &names).await;
                }
                Err(error) => eprintln!("[rsynapse-steward/namer] failed: {error}"),
            }
        }
        Ok(())
    }

    /// Name the focused workspace on first selection only. A workspace that
    /// already has a name record (any source, including manual) is a cheap
    /// noop, so names are stable and our own writes never retrigger work.
    async fn maybe_name(
        &self,
        focused: Option<u64>,
        projects: &[RelationRecord],
        names: &[RelationRecord],
    ) {
        let Some(workspace) = focused else {
            return;
        };
        let subject = workspace_subject(workspace);
        if names.iter().any(|record| record.subject == subject) {
            return;
        }
        let taken: HashSet<String> = names.iter().filter_map(name_of).collect();
        let project = project_name_for(workspace, projects);
        let Some((name, source)) = desired_name(project, workspace, &taken) else {
            return;
        };
        self.set_name(&subject, &name, source).await;
    }

    async fn set_name(&self, subject: &RelationEndpoint, name: &str, source: &str) {
        let target = locus::RelationEndpoint::stable_key(relations::WORKSPACE_NAME_KIND, name);
        let metadata = HashMap::from([("source".to_owned(), source.to_owned())]);
        if let Err(error) = self
            .locus
            .set_one(subject.clone(), relations::WORKSPACE_NAME, target, metadata)
            .await
        {
            eprintln!("[rsynapse-steward/namer] set name failed: {error}");
        }
    }
}

/// First-write decision for a nameless focused workspace: project display
/// name, heuristic suggestion, or a stable random word. Pure for tests.
fn desired_name(
    project: Option<String>,
    workspace: u64,
    taken: &HashSet<String>,
) -> Option<(String, &'static str)> {
    project
        .map(|name| (name, SOURCE_PROJECT))
        .or_else(|| heuristic_name().map(|name| (name, "heuristic")))
        .or_else(|| Some((random_name(workspace, taken), SOURCE_RANDOM)))
}

fn project_name_for(workspace: u64, projects: &[RelationRecord]) -> Option<String> {
    let subject = workspace_subject(workspace);
    projects.iter().find_map(|record| {
        if record.subject != subject {
            return None;
        }
        record
            .metadata
            .get("display-main")
            .cloned()
            .filter(|name| !name.trim().is_empty())
            .or_else(|| {
                record
                    .metadata
                    .get("name")
                    .cloned()
                    .filter(|name| !name.trim().is_empty())
            })
            .or_else(|| {
                record.metadata.get("path").and_then(|path| {
                    std::path::Path::new(path)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .map(str::to_owned)
                })
            })
    })
}

fn name_of(record: &RelationRecord) -> Option<String> {
    match &record.target {
        RelationEndpoint::StableKey { kind, id } if kind == relations::WORKSPACE_NAME_KIND => {
            (!id.trim().is_empty()).then(|| id.clone())
        }
        _ => None,
    }
}

/// Heuristic names for project-less workspaces (window mix, app kinds, …).
/// Stub seam for later: ML or rule suggestions plug in here.
fn heuristic_name() -> Option<String> {
    None
}

fn workspace_subject(id: u64) -> RelationEndpoint {
    RelationEndpoint::stable_key(locus::keys::NIRI_WORKSPACE_ID, id.to_string())
}

fn random_name(workspace: u64, taken: &HashSet<String>) -> String {
    for salt in 0..ADJECTIVES.len() * NOUNS.len() {
        let index = (workspace as usize)
            .wrapping_mul(31)
            .wrapping_add(salt * 101) ;
        let name = format!(
            "{}-{}",
            ADJECTIVES[index % ADJECTIVES.len()],
            NOUNS[(index / ADJECTIVES.len()) % NOUNS.len()]
        );
        if !taken.contains(&name) {
            return name;
        }
    }
    format!("ws-{workspace}")
}

const ADJECTIVES: &[&str] = &[
    "brave", "calm", "deft", "eager", "faint", "grand", "hasty", "idle", "jolly", "keen",
    "lucid", "merry", "noble", "odd", "proud", "quick", "rusty", "sandy", "tidy", "umbral",
    "vivid", "witty", "young", "zesty",
];

const NOUNS: &[&str] = &[
    "otter", "fox", "heron", "mole", "newt", "owl", "panda", "quail", "raven", "stoat",
    "tapir", "urchin", "vole", "wren", "yak", "zebra", "acorn", "birch", "cedar", "dune",
    "ember", "fjord", "grove", "harbor",
];

fn focused_workspace_id() -> Observable<Option<u64>> {
    let descriptor = ObjectDescriptor::parse(
        Bus::Session,
        niri_dbus::BUS_NAME,
        niri_dbus::ROOT_PATH,
        niri_dbus::ROOT_INTERFACE,
    )
    .expect("niri root descriptor should be valid");
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

    #[test]
    fn random_names_are_stable_and_unique() {
        let taken = HashSet::new();
        let first = random_name(42, &taken);
        assert_eq!(random_name(42, &taken), first);
        assert!(first.contains('-'));

        let mut taken = HashSet::from([first.clone()]);
        let second = random_name(43, &taken);
        assert_ne!(second, first);
        taken.insert(second.clone());
        assert!(!taken.contains(&random_name(44, &taken)));
    }

    #[test]
    fn project_names_prefer_display_main() {
        let subject = workspace_subject(7);
        let record = RelationRecord {
            subject: subject.clone(),
            relation: relations::WORKSPACE_PROJECT.to_owned(),
            target: locus::RelationEndpoint::stable_key("org.rsynapse.project.path", "/x"),
            metadata: HashMap::from([
                ("name".to_owned(), "coro-ncm".to_owned()),
                ("display-main".to_owned(), "coro-uiq".to_owned()),
            ]),
            created_at_unix_ms: 0,
            updated_at_unix_ms: 0,
        };
        assert_eq!(
            project_name_for(7, std::slice::from_ref(&record)),
            Some("coro-uiq".to_owned())
        );
        assert_eq!(project_name_for(8, std::slice::from_ref(&record)), None);
    }

    #[test]
    fn first_write_prefers_project_then_random() {
        let taken = HashSet::new();
        assert_eq!(
            desired_name(Some("coro-uiq".to_owned()), 7, &taken),
            Some(("coro-uiq".to_owned(), SOURCE_PROJECT))
        );
        let (name, source) = desired_name(None, 7, &taken).expect("random fallback");
        assert_eq!(source, SOURCE_RANDOM);
        assert!(name.contains('-'));
    }
}
