use shell_core::source::{Observable, github, rx::Observable as _};

use super::{Scope, category::Category, scope};

/// Structural list descriptor for one pull request row.
///
/// Only the URL (row identity) and the category (display policy) are stored
/// here. Live fields such as title, detail, unseen state and revision are
/// subscribed by the row component itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PrEntry {
    pub url: String,
    pub category: Category,
}

impl PrEntry {
    pub(super) fn key(&self) -> String {
        self.url.clone()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct GitHubView {
    pub available: bool,
    pub rows: Vec<PrEntry>,
    pub unseen: usize,
}

impl GitHubView {
    pub fn visible(&self) -> bool {
        self.available && !self.rows.is_empty()
    }
    pub fn unseen_count(&self) -> usize {
        self.unseen
    }
    pub fn count(&self) -> String {
        if self.available {
            self.rows.len().to_string()
        } else {
            "—".into()
        }
    }

    pub fn tooltip(&self, category: Category) -> String {
        if self.available {
            format!(
                "{}: {} · {} unseen\nClick to see pull requests",
                category.title(),
                self.rows.len(),
                self.unseen_count()
            )
        } else {
            format!("{}\nGitHub data is unavailable", category.title())
        }
    }

    pub fn empty_message(&self) -> &'static str {
        if self.available {
            "No matching pull requests."
        } else {
            "GitHub data is unavailable."
        }
    }
}

pub(super) fn view(
    category: Category,
    prs: Option<Vec<github::PullRequestActivity>>,
) -> GitHubView {
    let Some(prs) = prs else {
        return GitHubView::default();
    };
    let matching: Vec<_> = prs
        .into_iter()
        .filter(|item| Category::for_pr(&item.pr) == category)
        .collect();
    let unseen = matching.iter().filter(|item| item.unseen).count();
    let rows = matching
        .into_iter()
        .map(|item| PrEntry {
            url: item.pr.url,
            category,
        })
        .collect();
    GitHubView {
        available: true,
        rows,
        unseen,
    }
}

/// Display view for one row, derived from that PR's live activity snapshot.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct PrRowView {
    pub title: String,
    pub detail: String,
    pub revision: String,
    pub unseen: bool,
    pub available: bool,
}

impl PrRowView {
    pub fn tooltip(&self, url: &str) -> String {
        if self.available {
            format!(
                "{}\n{}\nLeft-click to open · Right-click to mark seen",
                self.title, url
            )
        } else {
            format!("{url}\nThis pull request is no longer available")
        }
    }
}

/// Live row projection. The URL is the stable identity; everything else here
/// may change without the parent rebuilding the list.
pub(super) fn row_view(
    category: Category,
    entry: Option<github::PullRequestActivity>,
) -> PrRowView {
    let Some(item) = entry else {
        return PrRowView::default();
    };
    PrRowView {
        title: item.pr.title.clone(),
        detail: detail_text(category, &item.pr),
        revision: item.revision,
        unseen: item.unseen,
        available: true,
    }
}

fn detail_text(category: Category, pr: &github::PullRequest) -> String {
    let mut detail = format!("{} #{} · {}", pr.repo, pr.number, status_name(category, pr));
    if category.is_activity() {
        detail.push_str(" · ");
        detail.push_str(category.title());
    }
    if pr.unresolved_threads > 0 {
        detail.push_str(&format!(
            " · {} unresolved review {}",
            pr.unresolved_threads,
            if pr.unresolved_threads == 1 {
                "thread"
            } else {
                "threads"
            }
        ));
    }
    detail
}

fn status_name(category: Category, pr: &github::PullRequest) -> &'static str {
    use github::PrStatus::*;
    if category == Category::Failed
        || (category.is_activity() && matches!(pr.checks.as_deref(), Some("FAILURE" | "ERROR")))
    {
        return if pr.status == MergeFailed {
            "Merge pipeline failed"
        } else {
            "CI failed"
        };
    }
    match pr.status {
        Merged => "Merged",
        Merging => "Merging",
        MergeFailed => "Merge pipeline failed",
        Draft => "Draft",
        Conflicts => "Conflicts",
        ChangesRequested => "Changes requested",
        OwnersRequired => "Needs review",
        CiFailed => "CI failed",
        CiPending => "CI pending",
        Approved => "Approved",
        NeedsReview => "Needs review",
    }
}

pub(super) fn status(scope: Scope, category: Category) -> Observable<GitHubView> {
    // Count and rows come from the same snapshot, so separate Summary/PR signals
    // cannot temporarily put a different count on the button than in its list.
    scope::for_category(scope, category)
        .map(move |prs| view(category, prs))
        .distinct_until_changed()
        .box_it()
}

#[cfg(test)]
mod test;
