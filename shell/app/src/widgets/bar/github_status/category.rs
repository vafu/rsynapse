use shell_core::source::github::{PrStatus, PullRequest};

use crate::widgets::nerd_icon::{NerdIcon, cod, fa, md};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Category {
    Merging,
    RecentlyMerged,
    Failed,
    Pending,
    Approved,
    ToAddress,
    Conflicts,
    ChangesRequested,
    NeedsReview,
    Draft,
}

impl Category {
    pub const ALL: [Self; 10] = [
        Self::Merging,
        Self::RecentlyMerged,
        Self::Failed,
        Self::Pending,
        Self::Approved,
        Self::ToAddress,
        Self::Conflicts,
        Self::ChangesRequested,
        Self::NeedsReview,
        Self::Draft,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Merging => "Pull requests merging now",
            Self::RecentlyMerged => "Recently merged pull requests",
            Self::Failed => "Failed pull requests",
            Self::Pending => "Pull requests with CI pending",
            Self::Approved => "Approved pull requests",
            Self::ToAddress => "Pull requests with review threads to address",
            Self::Conflicts => "Pull requests with merge conflicts",
            Self::ChangesRequested => "Pull requests with changes requested",
            Self::NeedsReview => "Pull requests awaiting review",
            Self::Draft => "Draft pull requests",
        }
    }

    pub fn icon(self) -> NerdIcon {
        NerdIcon::new(match self {
            Self::Merging => cod::COD_GIT_MERGE,
            Self::RecentlyMerged => fa::FA_CHECK_DOUBLE,
            Self::Failed => md::MD_ALERT,
            Self::Pending => cod::COD_SYNC,
            Self::Approved => fa::FA_CIRCLE_CHECK,
            Self::ToAddress => fa::FA_COMMENT,
            Self::Conflicts => fa::FA_CODE_FORK,
            Self::ChangesRequested => fa::FA_PENCIL,
            Self::NeedsReview => fa::FA_EYE,
            Self::Draft => fa::FA_FILE_TEXT,
        })
    }

    pub fn css_class(self) -> &'static str {
        match self {
            Self::Merging => "github-merging",
            Self::RecentlyMerged => "github-merged",
            Self::Failed => "github-failed",
            Self::Pending => "github-pending",
            Self::Approved => "github-approved",
            Self::ToAddress => "github-to-address",
            Self::Conflicts => "github-conflicts",
            Self::ChangesRequested => "github-changes-requested",
            Self::NeedsReview => "github-needs-review",
            Self::Draft => "github-draft",
        }
    }

    /// Assign exactly one pile. Lifecycle and explicit blockers take priority;
    /// Failed check rollups can be masked by the service's approval status, so
    /// check them before unresolved feedback and waiting/ready states.
    pub fn for_pr(pr: &PullRequest) -> Self {
        match pr.status {
            PrStatus::Merged => Self::RecentlyMerged,
            PrStatus::Merging => Self::Merging,
            PrStatus::Conflicts => Self::Conflicts,
            PrStatus::ChangesRequested => Self::ChangesRequested,
            PrStatus::CiFailed | PrStatus::MergeFailed => Self::Failed,
            _ if matches!(pr.checks.as_deref(), Some("FAILURE" | "ERROR")) => Self::Failed,
            _ if pr.unresolved_threads > 0 => Self::ToAddress,
            PrStatus::CiPending => Self::Pending,
            PrStatus::Approved => Self::Approved,
            PrStatus::OwnersRequired | PrStatus::NeedsReview => Self::NeedsReview,
            PrStatus::Draft => Self::Draft,
        }
    }
}
