//! Explicit calibration inputs, not a composite productivity score.
pub mod cli;
mod tracker;
pub use tracker::Tracker;
pub use tracker::now as tracker_now;

use crate::{
    agent_metrics::Session,
    focus::FocusState,
    relations::{self, LocusClient},
};
use clap::ValueEnum;
use data_sources::{Observable, rx::Observable as _};
use locus::{RelationEndpoint, RelationRecord};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

pub const CLASSIFICATION: &str = "org.rsynapse.context.classification";
pub const WORKSPACE_CONTEXT: &str = "org.rsynapse.workspace.context";
const SUMMARY: &str = "org.rsynapse.productivity.summary";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Classification {
    #[default]
    Unclassified,
    Work,
    Personal,
}

#[derive(Clone, Default, PartialEq)]
pub struct Policy {
    pub workspaces: HashMap<String, Classification>,
    pub projects: BTreeMap<PathBuf, Classification>,
    workspace_projects: HashMap<String, PathBuf>,
}

pub fn policies(locus: &LocusClient) -> Observable<Policy> {
    shell_rx_macros::combine_latest!(
        relations::records(locus.clone(), CLASSIFICATION),
        relations::records(locus.clone(), relations::WORKSPACE_PROJECT)
        => |(classes, projects)| {
            Policy::from_records(&classes, &projects)
        },
    )
    .distinct_until_changed()
    .box_it()
}

impl Policy {
    fn from_records(classes: &[RelationRecord], projects: &[RelationRecord]) -> Self {
        let mut policy = Self::default();
        for record in classes {
            let Some(class) = record
                .metadata
                .get("class")
                .and_then(|s| serde_json::from_str::<Classification>(s).ok())
            else {
                continue;
            };
            if let RelationEndpoint::StableKey { kind, id } = &record.subject {
                if kind == WORKSPACE_CONTEXT {
                    policy.workspaces.insert(id.clone(), class);
                }
                if kind == locus::keys::PROJECT_PATH {
                    policy.projects.insert(PathBuf::from(id), class);
                }
            }
        }
        for record in projects {
            if let (
                RelationEndpoint::StableKey { kind, id },
                RelationEndpoint::StableKey {
                    kind: target_kind,
                    id: target,
                },
            ) = (&record.subject, &record.target)
            {
                if kind == locus::keys::WORKSPACE_ID && target_kind == locus::keys::PROJECT_PATH {
                    policy
                        .workspace_projects
                        .insert(id.clone(), PathBuf::from(target));
                }
            }
        }
        policy
    }

    fn project_class(&self, path: Option<&Path>) -> Classification {
        path.and_then(|path| {
            self.projects
                .iter()
                .filter(|(root, _)| path.starts_with(root))
                .max_by_key(|(root, _)| root.components().count())
                .map(|(_, class)| *class)
        })
        .unwrap_or_default()
    }
    fn classify(&self, workspace: Option<&str>, project: Option<&Path>) -> Classification {
        let workspace = workspace
            .and_then(|name| self.workspaces.get(name))
            .copied()
            .unwrap_or_default();
        let project = self.project_class(project);
        // Personal classification always vetoes work credit, even with a goal link.
        if workspace == Classification::Personal || project == Classification::Personal {
            Classification::Personal
        } else if workspace == Classification::Work || project == Classification::Work {
            Classification::Work
        } else {
            Classification::Unclassified
        }
    }
    pub fn human_class(&self, focus: &FocusState) -> Classification {
        self.classify(
            focus.workspace_name.as_deref(),
            focus
                .workspace_id
                .as_ref()
                .and_then(|id| self.workspace_projects.get(id))
                .map(PathBuf::as_path),
        )
    }
    pub fn eligible_agent(&self, session: &Session, _day: &str) -> bool {
        if session.subagent
            || !matches!(session.state.as_str(), "thinking" | "tool-use")
            || session.window.is_none()
        {
            return false;
        }
        let class = self.classify(
            Some(&session.workspace),
            (!session.cwd.is_empty()).then(|| Path::new(&session.cwd)),
        );
        class == Classification::Work
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn most_specific_project_class_and_personal_veto() {
        let policy = Policy {
            projects: BTreeMap::from([
                ("/work".into(), Classification::Work),
                ("/work/private".into(), Classification::Personal),
            ]),
            workspaces: HashMap::from([("work".into(), Classification::Work)]),
            ..Policy::default()
        };
        assert_eq!(
            policy.classify(Some("work"), Some(Path::new("/work/private/src"))),
            Classification::Personal
        );
        assert_eq!(
            policy.classify(None, Some(Path::new("/work/src"))),
            Classification::Work
        );
        assert_eq!(
            policy.classify(None, Some(Path::new("/work-other"))),
            Classification::Unclassified
        );
    }
}
