use shell_core::source::github::{PrStatus, PullRequest, ReviewActivity};

use crate::widgets::nerd_icon::{NerdIcon, cod, fa, md};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Category {
    InitialReview,
    ReReview,
    MyChangesRequested,
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
    pub const ALL: [Self; 13] = [
        Self::InitialReview,
        Self::ReReview,
        Self::MyChangesRequested,
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
            Self::InitialReview => "Needs your initial review",
            Self::ReReview => "Needs your re-review",
            Self::MyChangesRequested => "Changes requested by you",
            Self::Merging => "Pull requests merging now",
            Self::RecentlyMerged => "Recently merged pull requests",
            Self::Failed => "Failed pull requests",
            Self::Pending => "Pull requests with CI pending",
            Self::Approved => "Approved pull requests",
            Self::ToAddress => "Pull requests with review threads to address",
            Self::Conflicts => "Pull requests with merge conflicts",
            Self::ChangesRequested => "Pull requests with changes requested",
            Self::NeedsReview => "Pull requests awaiting approval",
            Self::Draft => "Draft pull requests",
        }
    }

    pub fn icon(self) -> NerdIcon {
        NerdIcon::new(match self {
            Self::InitialReview => fa::FA_USER_CHECK,
            Self::ReReview => cod::COD_SYNC,
            Self::MyChangesRequested => fa::FA_PENCIL,
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
            Self::InitialReview => "github-initial-review",
            Self::ReReview => "github-re-review",
            Self::MyChangesRequested => "github-my-changes-requested",
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

    /// Assign exactly one pile. Open incoming requests are tasks for the viewer;
    /// other PRs use author-side lifecycle/blocker precedence. Raw failed checks
    /// outrank waiting states even if the service reports an approval requirement.
    pub fn for_pr(pr: &PullRequest) -> Self {
        // Incoming requests are work for the viewer, not author-side CI/review
        // blockers. Keep them in one dedicated pile while the request is active.
        if pr.status != PrStatus::Merged {
            match pr
                .review_activity
                .or_else(|| pr.review_requested.then_some(ReviewActivity::InitialReview))
            {
                Some(ReviewActivity::InitialReview) => return Self::InitialReview,
                Some(ReviewActivity::ReReview) => return Self::ReReview,
                Some(ReviewActivity::ChangesRequested) => return Self::MyChangesRequested,
                None => {}
            }
        }
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

    pub fn is_activity(self) -> bool {
        matches!(
            self,
            Self::InitialReview | Self::ReReview | Self::MyChangesRequested
        )
    }
}
