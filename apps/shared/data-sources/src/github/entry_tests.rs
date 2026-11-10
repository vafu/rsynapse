use super::{PrStatus, PullRequest, PullRequestActivity, find_entry};

fn activity(url: &str, title: &str, revision: &str) -> PullRequestActivity {
    PullRequestActivity {
        pr: PullRequest {
            title: title.into(),
            url: url.into(),
            number: 1,
            repo: "team/repo".into(),
            status: PrStatus::Approved,
            cool_status: None,
            merged_at: None,
            review_decision: None,
            mergeable: None,
            merge_state_status: None,
            checks: None,
            updated_at: String::new(),
            unresolved_threads: 0,
            latest_review: None,
            labels: Vec::new(),
            review_requested: false,
            review_activity: None,
        },
        revision: revision.into(),
        unseen: false,
    }
}

#[test]
fn entry_projection_selects_by_url_and_ignores_other_pull_requests() {
    let first = activity("https://example.test/pull/1", "First", "rev-1");
    let second = activity("https://example.test/pull/2", "Second", "rev-2");
    let rows = vec![first.clone(), second.clone()];

    assert_eq!(
        find_entry(&rows, "https://example.test/pull/2"),
        Some(second)
    );
    assert_eq!(find_entry(&rows, "https://example.test/pull/3"), None);
}

#[test]
fn a_changed_title_for_the_same_url_is_visible_to_the_row_source() {
    let before = activity("https://example.test/pull/1", "Old title", "rev-1");
    let after = activity("https://example.test/pull/1", "New title", "rev-2");

    // The row's structural identity is the URL, so this is an in-place update
    // rather than a different row.
    assert_eq!(before.pr.url, after.pr.url);
    assert_ne!(before, after);
    assert_eq!(
        find_entry(&[after.clone()], &after.pr.url),
        Some(after.clone())
    );
    // Only the live snapshot changes; the URL-derived key is stable.
    assert_eq!(
        find_entry(&[before], &after.pr.url).unwrap().pr.url,
        after.pr.url
    );
}

#[test]
fn acknowledgments_update_the_entry_without_changing_its_url() {
    let mut unseen = activity("https://example.test/pull/1", "Title", "rev-1");
    unseen.unseen = true;
    let mut seen = unseen.clone();
    seen.unseen = false;

    assert_eq!(seen.pr.url, unseen.pr.url);
    assert_ne!(seen, unseen);
    assert_eq!(find_entry(&[seen.clone()], &unseen.pr.url), Some(seen));
}
