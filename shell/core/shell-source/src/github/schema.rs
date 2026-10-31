//! Rsynapse's client-side schema for github-dbus JSON properties and signals.
//!
//! These are wire records, independent of the producer's Rust crates. Keep
//! field names, enum spellings and defaults compatible with the service.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Status-keyed counts flattened alongside the total in the JSON object.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrSummary {
    #[serde(flatten)]
    pub counts: HashMap<PrStatus, u32>,
    pub total: u32,
}

/// A pull request as reported by the service, including raw status details.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequest {
    pub title: String,
    pub url: String,
    pub number: u64,
    pub repo: String,
    pub status: PrStatus,
    /// The producer's merge-pipeline status text; retain its existing wire key.
    pub cool_status: Option<String>,
    pub merged_at: Option<String>,
    pub review_decision: Option<String>,
    pub mergeable: Option<String>,
    pub merge_state_status: Option<String>,
    pub checks: Option<String>,
    pub updated_at: String,
    #[serde(default)]
    pub unresolved_threads: u32,
    pub latest_review: Option<ReviewInfo>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub review_requested: bool,
    #[serde(default)]
    pub review_activity: Option<ReviewActivity>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewInfo {
    pub state: String,
    pub author: String,
    pub submitted_at: String,
}

/// Producer-derived statuses, serialized as snake_case strings.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrStatus {
    Merged,
    Merging,
    MergeFailed,
    Draft,
    Conflicts,
    ChangesRequested,
    OwnersRequired,
    CiFailed,
    CiPending,
    Approved,
    NeedsReview,
}

/// A coherent PR snapshot plus its opaque revision and acknowledgment state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestActivity {
    pub pr: PullRequest,
    pub revision: String,
    pub unseen: bool,
}

/// Explicit panel counts. Older producers can omit newly added counters.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PrOverview {
    pub open: u32,
    pub merging: u32,
    pub failed: u32,
    pub ci_pending: u32,
    pub approved: u32,
    pub needs_review: u32,
    pub draft: u32,
    pub conflicts: u32,
    pub changes_requested: u32,
    pub owners_required: u32,
    pub ci_failed: u32,
    pub merge_failed: u32,
    pub merged: u32,
    pub to_address: u32,
    pub review_requested: u32,
    pub initial_review: u32,
    pub re_review: u32,
    pub changes_requested_by_me: u32,
}

/// Viewer-specific review workflow, independent of local acknowledgment state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewActivity {
    InitialReview,
    ReReview,
    ChangesRequested,
}

/// One notification-thread update. Reasons and subject kinds are open strings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationEvent {
    pub thread_id: String,
    pub reason: String,
    pub kind: String,
    pub title: String,
    pub repo: String,
    pub url: Option<String>,
    pub updated_at: String,
}

#[cfg(test)]
mod tests;
