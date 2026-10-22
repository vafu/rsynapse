use shell_core::source::{Observable, github, rx::Observable as _};

use super::{Scope, category::Category, scope};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PrEntry {
    pub title: String,
    pub url: String,
    pub detail: String,
    pub revision: String,
    pub unseen: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct GitHubView {
    pub available: bool,
    pub rows: Vec<PrEntry>,
}

impl GitHubView {
    pub fn visible(&self) -> bool {
        self.available && !self.rows.is_empty()
    }
    pub fn unseen_count(&self) -> usize {
        self.rows.iter().filter(|row| row.unseen).count()
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
    let rows = prs
        .into_iter()
        .filter(|item| Category::for_pr(&item.pr) == category)
        .map(|item| {
            let mut detail = format!(
                "{} #{} · {}",
                item.pr.repo,
                item.pr.number,
                status_name(category, item.pr.status)
            );
            if item.pr.unresolved_threads > 0 {
                detail.push_str(&format!(
                    " · {} unresolved review {}",
                    item.pr.unresolved_threads,
                    if item.pr.unresolved_threads == 1 {
                        "thread"
                    } else {
                        "threads"
                    }
                ));
            }
            PrEntry {
                detail,
                title: item.pr.title,
                url: item.pr.url,
                revision: item.revision,
                unseen: item.unseen,
            }
        })
        .collect();
    GitHubView {
        available: true,
        rows,
    }
}

fn status_name(category: Category, status: github::PrStatus) -> &'static str {
    use github::PrStatus::*;
    if category == Category::Failed {
        return if status == MergeFailed {
            "Merge pipeline failed"
        } else {
            "CI failed"
        };
    }
    match status {
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
    scope::inventory(scope)
        .map(move |prs| view(category, prs))
        .distinct_until_changed()
        .box_it()
}

#[cfg(test)]
mod test;
