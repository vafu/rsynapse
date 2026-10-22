use super::*;
use crate::widgets::bar::{
    github_status::{category::Category, source::view},
    selected_project::ProjectView,
};
use github::{PrStatus, PullRequest, PullRequestActivity};

mod reactive;

fn pr(number: u64, status: PrStatus, labels: &[&str]) -> PullRequestActivity {
    PullRequestActivity {
        pr: PullRequest {
            review_activity: None,
            review_requested: false,
            title: format!("PR {number}"),
            url: format!("https://example.test/repo/pull/{number}"),
            number,
            repo: "example/repo".into(),
            labels: labels.iter().map(|s| (*s).into()).collect(),
            status,
            cool_status: None,
            merged_at: None,
            review_decision: None,
            mergeable: None,
            merge_state_status: None,
            checks: None,
            updated_at: String::new(),
            unresolved_threads: 0,
            latest_review: None,
        },
        revision: format!("opaque-revision-{number}"),
        unseen: true,
    }
}

fn urls(prs: &[PullRequestActivity]) -> Option<Vec<String>> {
    Some(prs.iter().map(|item| item.pr.url.clone()).collect())
}

#[test]
fn listener_membership_is_authoritative_without_local_label_matching() {
    let prs = vec![
        pr(1, PrStatus::Approved, &["team/mobile"]),
        pr(2, PrStatus::Draft, &["feature/foo", "team/mobile"]),
        pr(3, PrStatus::NeedsReview, &["feature/foo"]),
        pr(4, PrStatus::CiFailed, &["team/mobile-extra"]),
        pr(5, PrStatus::Merging, &["TEAM/mobile"]),
        pr(6, PrStatus::Merged, &[]),
        pr(7, PrStatus::Approved, &[" team/mobile"]),
    ];
    // Include a URL whose local metadata does not match, and exclude one whose
    // metadata does match: listener membership, not local label checks, wins.
    let expected = vec![prs[0].clone(), prs[2].clone(), prs[3].clone()];
    assert_eq!(filter_inventory(Some(prs), urls(&expected)), Some(expected),);
}

#[test]
fn global_unavailable_and_zero_matches_are_distinct() {
    let prs = Some(vec![pr(1, PrStatus::Merged, &[])]);
    assert_eq!(
        filter_inventory(prs.clone(), urls(prs.as_ref().unwrap())),
        prs
    );
    assert_eq!(filter_inventory(prs.clone(), None), None);
    assert_eq!(filter_inventory(None, Some(vec![])), None);
    let empty = filter_inventory(prs, Some(vec![]));
    assert_eq!(empty, Some(vec![]));
    for category in Category::ALL {
        let bucket = view(category, empty.clone());
        assert!(bucket.available);
        assert_eq!(bucket.count(), "0");
        assert_eq!(bucket.unseen_count(), 0);
        assert!(!bucket.visible());
    }
}

#[test]
fn filtered_categories_conserve_inventory_and_preserve_failure_guard() {
    let mut prs = vec![
        pr(1, PrStatus::Merged, &["a"]),
        pr(2, PrStatus::Merging, &["b"]),
        pr(3, PrStatus::OwnersRequired, &["a", "b"]),
        pr(4, PrStatus::Conflicts, &["b"]),
        pr(5, PrStatus::ChangesRequested, &["a"]),
        pr(6, PrStatus::Approved, &["b"]),
        pr(7, PrStatus::Draft, &["a"]),
        pr(8, PrStatus::CiPending, &["b"]),
        pr(9, PrStatus::NeedsReview, &["a"]),
        pr(10, PrStatus::Approved, &["a"]),
        pr(11, PrStatus::CiFailed, &["a"]),
        pr(12, PrStatus::CiPending, &["a"]),
        pr(13, PrStatus::CiPending, &["a"]),
        pr(14, PrStatus::CiFailed, &["unrelated"]),
    ];
    prs[10].pr.review_requested = true;
    prs[11].pr.review_requested = true;
    prs[11].pr.review_activity = Some(github::ReviewActivity::ReReview);
    prs[12].pr.review_activity = Some(github::ReviewActivity::ChangesRequested);
    prs[2].pr.checks = Some("FAILURE".into());
    prs[2].pr.unresolved_threads = 2;
    prs[5].pr.unresolved_threads = 1;
    prs[8].unseen = false;
    let filtered = filter_inventory(Some(prs.clone()), urls(&prs[..13]));
    assert_eq!(filtered, Some(prs[..13].to_vec()));
    let mut all_rows = Vec::new();
    let mut unseen = 0;
    for category in Category::ALL {
        let bucket = view(category, filtered.clone());
        assert_eq!(bucket.count(), bucket.rows.len().to_string());
        assert_eq!(bucket.rows.len(), 1, "{category:?}");
        unseen += bucket.unseen_count();
        all_rows.extend(bucket.rows);
    }
    assert_eq!(all_rows.len(), 13);
    assert_eq!(unseen, 12);
    for original in &prs[..13] {
        let rows: Vec<_> = all_rows
            .iter()
            .filter(|row| row.url == original.pr.url)
            .collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].revision, original.revision);
        assert_eq!(rows[0].unseen, original.unseen);
    }
    assert_eq!(view(Category::Failed, filtered).rows[0].url, prs[2].pr.url);
}

#[test]
fn empty_workspace_with_project_still_has_project_scope() {
    // No path, title, Git metadata, or windows are needed to retain association.
    let workspace = SelectedWorkspaceView {
        workspace_id: Some("empty-workspace".into()),
        visible: true,
        project: Some(ProjectView {
            project_id: Some("p".into()),
            visible: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(
        project_scope(Some(workspace.clone())),
        ProjectScope::Project("p".into())
    );
    let unknown_id = SelectedWorkspaceView {
        project: Some(ProjectView::default()),
        ..workspace.clone()
    };
    assert_eq!(project_scope(Some(unknown_id)), ProjectScope::Unavailable);
    assert_eq!(project_scope(None), ProjectScope::Unavailable);
    assert_eq!(
        project_scope(Some(SelectedWorkspaceView {
            project: None,
            ..workspace
        })),
        ProjectScope::Global
    );
}
