//! Public project-management records. No desktop, locus, or UI dependencies.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use zbus::zvariant::Type;

pub const BUS_NAME: &str = "org.rsynapse.Proj";
pub const ROOT_PATH: &str = "/org/rsynapse/Proj";
pub const MANAGER_INTERFACE: &str = "org.rsynapse.Proj.Manager1";
pub const PROJECT_INTERFACE: &str = "org.rsynapse.Proj.Project1";
pub const CHECKOUT_INTERFACE: &str = "org.rsynapse.Proj.Checkout1";
pub const GOAL_INTERFACE: &str = "org.rsynapse.Proj.Goal1";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
pub struct GitStatus {
    pub staged: u32,
    pub unstaged: u32,
    pub untracked: u32,
    pub ahead: u32,
    pub behind: u32,
    pub merging: bool,
    pub rebasing: bool,
    pub stashes: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct ProjectInfo {
    pub id: String,
    pub name: String,
    pub cwd: String,
    #[serde(default)]
    pub checkout_id: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub icon_origin: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct CheckoutInfo {
    pub id: String,
    pub root_path: String,
    pub branch: String,
    pub git_status: GitStatus,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct RemovedProjectInfo {
    pub project: ProjectInfo,
    pub checkouts: Vec<CheckoutInfo>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
pub struct GoalInfo {
    pub id: String,
    pub date: String,
    pub title: String,
    pub kind: String,
    pub project: String,
    pub success: String,
    pub priority: String,
    pub status: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub parent_id: String,
    #[serde(default)]
    pub note_path: String,
    #[serde(default)]
    pub project_id: String,
}
impl GoalInfo {
    pub fn key(&self) -> String {
        goal_key(&self.date, &self.id)
    }
    pub fn object_path(&self) -> String {
        goal_path(&self.date, &self.id)
    }
}
pub fn goal_key(date: &str, id: &str) -> String {
    if date.is_empty() {
        id.to_owned()
    } else {
        format!("{date}/{id}")
    }
}
impl From<Goal> for GoalInfo {
    fn from(g: Goal) -> Self {
        let label = |v: serde_json::Value| v.as_str().unwrap().to_owned();
        Self {
            id: g.id,
            date: g.date,
            title: g.title,
            kind: label(serde_json::to_value(g.kind).unwrap()),
            project: g
                .project
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            success: g.success,
            priority: label(serde_json::to_value(g.priority).unwrap()),
            status: label(serde_json::to_value(g.status).unwrap()),
            description: g.description,
            parent_id: g.parent_id,
            note_path: g.note_path,
            project_id: g.project_id,
        }
    }
}
impl TryFrom<GoalInfo> for Goal {
    type Error = String;
    fn try_from(g: GoalInfo) -> Result<Self, String> {
        let goal = Self {
            id: g.id,
            date: g.date,
            title: g.title,
            kind: serde_json::from_value(serde_json::Value::String(g.kind))
                .map_err(|e| e.to_string())?,
            project: (!g.project.is_empty()).then(|| PathBuf::from(g.project)),
            success: g.success,
            priority: serde_json::from_value(serde_json::Value::String(g.priority))
                .map_err(|e| e.to_string())?,
            status: serde_json::from_value(serde_json::Value::String(g.status))
                .map_err(|e| e.to_string())?,
            description: g.description,
            parent_id: g.parent_id,
            note_path: g.note_path,
            project_id: g.project_id,
        };
        goal.validate()?;
        Ok(goal)
    }
}
pub fn object_path(kind: &str, id: &str) -> String {
    if matches!(kind, "Projects" | "Checkouts")
        && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return format!(
            "{ROOT_PATH}/{kind}/{}{id}",
            if kind == "Projects" { "p" } else { "c" }
        );
    }
    let safe: String = id.as_bytes().iter().map(|b| format!("{b:02x}")).collect();
    format!("{ROOT_PATH}/{kind}/n{safe}")
}
pub fn goal_path(date: &str, id: &str) -> String {
    if date.is_empty() {
        let safe: String = id.as_bytes().iter().map(|b| format!("{b:02x}")).collect();
        return format!("{ROOT_PATH}/Goals/g{safe}");
    }
    object_path("Goals", &format!("{date}_{id}"))
}

pub const DAY_GOAL: &str = "org.rsynapse.day.goal";
pub const GOAL_KIND: &str = "org.rsynapse.goal";
pub const AGENT_GOAL: &str = "org.rsynapse.agent.goal";
pub const DAY_KIND: &str = "org.rsynapse.calendar-day";
pub const WORKDAY_TARGETS: &str = "org.rsynapse.workday.targets";
pub const PREFERENCES_KIND: &str = "org.rsynapse.session.preferences";

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkdayTargets {
    pub span_hours: f64,
    pub unlocked_hours: f64,
    pub active_hours: f64,
}
impl Default for WorkdayTargets {
    fn default() -> Self {
        Self {
            span_hours: 8.0,
            unlocked_hours: 6.0,
            active_hours: 4.0,
        }
    }
}
impl WorkdayTargets {
    pub fn validate(self) -> Result<(), String> {
        if [self.span_hours, self.unlocked_hours, self.active_hours]
            .iter()
            .any(|h| !h.is_finite() || *h < 0.0 || *h > 24.0)
        {
            return Err("Targets must be finite hours between 0 and 24".into());
        }
        if self.active_hours > self.unlocked_hours {
            return Err("Active target cannot exceed unlocked target".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    #[default]
    Planned,
    InProgress,
    Completed,
    Deferred,
}
impl Status {
    pub fn is_open(self) -> bool {
        matches!(self, Self::Planned | Self::InProgress)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Priority {
    Low,
    #[default]
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GoalType {
    Task,
    #[default]
    Outcome,
    Habit,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub date: String,
    pub title: String,
    #[serde(default)]
    pub kind: GoalType,
    pub project: Option<PathBuf>,
    pub success: String,
    pub priority: Priority,
    #[serde(default)]
    pub status: Status,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub parent_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note_path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub project_id: String,
}
impl Goal {
    pub fn key(&self) -> String {
        goal_key(&self.date, &self.id)
    }
    pub fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.id) {
            return Err("ID must be 1–64 ASCII letters, numbers, underscores or hyphens".into());
        }
        if !self.date.is_empty() && !valid_date(&self.date) {
            return Err("Date must be YYYY-MM-DD".into());
        }
        if self.title.trim().is_empty() || self.title.len() > 1024 {
            return Err("Title must be nonempty and at most 1024 bytes".into());
        }
        if (self.kind != GoalType::Task && self.success.trim().is_empty())
            || self.success.len() > 16384
        {
            return Err("Success criterion must be nonempty and at most 16384 bytes".into());
        }
        if self.description.len() > 65536 {
            return Err("Description must be at most 65536 bytes".into());
        }
        if self.kind == GoalType::Outcome && self.project.is_none() {
            return Err("Outcome goals require a project".into());
        }
        if self.project.as_ref().is_some_and(|p| !p.is_absolute()) {
            return Err("Project path must be absolute".into());
        }
        Ok(())
    }
}
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
pub fn valid_date(date: &str) -> bool {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .is_ok_and(|d| d.format("%Y-%m-%d").to_string() == date)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_record_boundaries_without_a_database() {
        let mut goal = Goal {
            id: "habit1".into(),
            date: "2026-10-07".into(),
            title: "Title".into(),
            kind: GoalType::Habit,
            project: None,
            success: "Criterion".into(),
            priority: Priority::High,
            status: Status::Planned,
            ..Default::default()
        };
        assert!(goal.validate().is_ok());
        goal.kind = GoalType::Outcome;
        assert!(goal.validate().is_err());
        goal.project = Some("/project".into());
        assert!(goal.validate().is_ok());
        goal.id = "../bad".into();
        assert!(goal.validate().is_err());
        assert!(!valid_date("2026-1-7"));
        assert!(!valid_date("2026-13-07"));
    }
    #[test]
    fn workday_defaults_allow_lunch_and_validate_targets() {
        let targets = WorkdayTargets::default();
        assert_eq!(targets.unlocked_hours, 6.0);
        assert!(targets.validate().is_ok());
        assert!(
            WorkdayTargets {
                active_hours: 7.0,
                ..targets
            }
            .validate()
            .is_err()
        );
        assert!(
            WorkdayTargets {
                span_hours: f64::INFINITY,
                ..targets
            }
            .validate()
            .is_err()
        );
    }
}
