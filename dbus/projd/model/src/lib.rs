//! Public project-management records. No desktop, locus, or UI dependencies.
use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;
pub mod api;
pub mod xdg;

pub const BUS_NAME: &str = "org.rsynapse.Proj";
pub const ROOT_PATH: &str = "/org/rsynapse/Proj";
pub const MANAGER_INTERFACE: &str = "org.rsynapse.Proj.Manager1";
pub const PROJECT_INTERFACE: &str = "org.rsynapse.Proj.Project1";
pub const CHECKOUT_INTERFACE: &str = "org.rsynapse.Proj.Checkout1";

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

#[cfg(test)]
mod tests {
    use super::*;
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
