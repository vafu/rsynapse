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
use locus::{RelationEndpoint, RelationRecord};
use serde::{Deserialize, Serialize};
use shell_source::{Observable, rx::Observable as _};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

pub const CLASSIFICATION: &str = "org.rsynapse.context.classification";
pub const WORKSPACE_CONTEXT: &str = "org.rsynapse.workspace.context";
pub use goal_model::{AGENT_GOAL, GOAL_KIND, Goal, GoalType};
#[cfg(test)]
use goal_model::{Priority, Status};
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
    pub goals: Vec<Goal>,
    pub links: HashMap<String, Vec<String>>,
    workspace_projects: HashMap<u64, PathBuf>,
}

pub fn policies(locus: &LocusClient) -> Observable<Policy> {
    shell_rx_macros::combine_latest!(
        relations::records(locus.clone(), CLASSIFICATION),
        shell_source::proj::goals(),
        relations::records(locus.clone(), AGENT_GOAL),
        relations::records(locus.clone(), relations::WORKSPACE_PROJECT)
        => |(classes, goals, links, projects)| {
            let mut policy=Policy::from_records(&classes,&[],&links,&projects);
            policy.goals=goals.into_iter().filter_map(|g|Goal::try_from(g).ok()).collect();
            policy
        },
    )
    .distinct_until_changed()
    .box_it()
}

pub fn goals(records: &[RelationRecord]) -> Vec<Goal> {
    records
        .iter()
        .filter_map(|r| {
            let goal: Goal = serde_json::from_str(r.metadata.get("goal")?).ok()?;
            let RelationEndpoint::StableKey { kind, id } = &r.target else {
                return None;
            };
            (kind == GOAL_KIND
                && *id == goal.key()
                && goal_model::valid_id(&goal.id)
                && chrono::NaiveDate::parse_from_str(&goal.date, "%Y-%m-%d").is_ok())
            .then_some(goal)
        })
        .collect()
}

impl Policy {
    fn from_records(
        classes: &[RelationRecord],
        records: &[RelationRecord],
        links: &[RelationRecord],
        projects: &[RelationRecord],
    ) -> Self {
        let mut policy = Self {
            goals: goals(records),
            ..Self::default()
        };
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
        for record in links {
            if let (
                RelationEndpoint::StableKey { kind, id },
                RelationEndpoint::StableKey {
                    kind: target_kind,
                    id: target,
                },
            ) = (&record.subject, &record.target)
            {
                if kind == locus::keys::AGENT_SESSION_ID && target_kind == GOAL_KIND {
                    policy
                        .links
                        .entry(id.clone())
                        .or_default()
                        .push(target.clone());
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
                if kind == locus::keys::NIRI_WORKSPACE_ID
                    && target_kind == locus::keys::PROJECT_PATH
                {
                    if let Ok(id) = id.parse() {
                        policy.workspace_projects.insert(id, PathBuf::from(target));
                    }
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
                .and_then(|id| self.workspace_projects.get(&id))
                .map(PathBuf::as_path),
        )
    }
    pub fn agent_goals(&self, session: &Session, day: &str) -> bool {
        let links = self
            .links
            .get(&format!("{}/{}", session.agent, session.session_id));
        self.goals.iter().any(|g| {
            g.date == day
                && g.status.is_open()
                && g.kind == GoalType::Outcome
                && ((g.project.as_ref().is_some_and(|root| {
                    !session.cwd.is_empty() && Path::new(&session.cwd).starts_with(root)
                })) || links.is_some_and(|links| links.contains(&g.key())))
        })
    }
    pub fn eligible_agent(&self, session: &Session, day: &str) -> bool {
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
        class != Classification::Personal
            && (class == Classification::Work || self.agent_goals(session, day))
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
    #[test]
    fn explicit_session_goal_link_is_scoped_by_day_and_status() {
        let goal = Goal {
            id: "g1".into(),
            date: "2026-10-05".into(),
            title: "title".into(),
            success: "criterion".into(),
            kind: GoalType::Outcome,
            project: Some("/other".into()),
            priority: Priority::High,
            status: Status::Planned,
        };
        let mut policy = Policy {
            links: HashMap::from([("codex/session".into(), vec![goal.key()])]),
            goals: vec![goal],
            ..Policy::default()
        };
        let session = Session {
            session_id: "session".into(),
            agent: "codex".into(),
            state: "thinking".into(),
            window: Some(1),
            ..Session::default()
        };
        assert!(policy.eligible_agent(&session, "2026-10-05"));
        assert!(!policy.eligible_agent(&session, "2026-10-06"));
        policy.goals[0].status = Status::Completed;
        assert!(!policy.eligible_agent(&session, "2026-10-05"));
    }

    #[test]
    fn habits_never_grant_agent_credit_and_old_goals_remain_outcomes() {
        let old = serde_json::json!({"id":"g1","date":"2026-10-05","title":"title","project":"/work","success":"criterion","priority":"high","status":"planned"});
        let mut goal: Goal = serde_json::from_value(old).unwrap();
        assert_eq!(goal.kind, GoalType::Outcome);
        assert_eq!(goal.project, Some(PathBuf::from("/work")));
        goal.kind = GoalType::Habit;
        let policy = Policy {
            links: HashMap::from([("codex/session".into(), vec![goal.key()])]),
            goals: vec![goal],
            ..Policy::default()
        };
        let session = Session {
            session_id: "session".into(),
            agent: "codex".into(),
            cwd: "/work/src".into(),
            state: "thinking".into(),
            window: Some(1),
            ..Session::default()
        };
        assert!(!policy.eligible_agent(&session, "2026-10-05"));
    }
}
