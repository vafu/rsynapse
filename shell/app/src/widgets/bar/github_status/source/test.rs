use super::*;
use github::{PrStatus, PullRequest, PullRequestActivity};

mod checks;
mod review_requests;

fn pr(number: u64, status: PrStatus) -> PullRequestActivity {
    PullRequestActivity {
        pr: PullRequest {
            review_activity: None,
            review_requested: false,
            title: format!("Title <{number}> & details"),
            url: format!("https://example.test/repo/pull/{number}"),
            number,
            repo: "example/repo".into(),
            labels: Vec::new(),
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
        revision: format!("revision-{number}"),
        unseen: status == PrStatus::Merged || matches!(number, 3 | 4),
    }
}

#[test]
fn each_counter_matches_its_filtered_link_list() {
    let mut prs = vec![
        pr(1, PrStatus::Merged),
        pr(2, PrStatus::Merging),
        pr(3, PrStatus::CiFailed),
        pr(4, PrStatus::MergeFailed),
        pr(5, PrStatus::Conflicts),
        pr(6, PrStatus::CiPending),
        pr(7, PrStatus::Approved),
        pr(8, PrStatus::OwnersRequired),
        pr(9, PrStatus::ChangesRequested),
        pr(10, PrStatus::NeedsReview),
        pr(11, PrStatus::Draft),
        pr(12, PrStatus::OwnersRequired),
        pr(13, PrStatus::CiFailed),
        pr(14, PrStatus::CiPending),
        pr(15, PrStatus::CiPending),
    ];
    prs[12].pr.review_requested = true;
    prs[12].pr.review_activity = Some(github::ReviewActivity::InitialReview);
    prs[13].pr.review_requested = true;
    prs[13].pr.review_activity = Some(github::ReviewActivity::ReReview);
    prs[14].pr.review_activity = Some(github::ReviewActivity::ChangesRequested);
    prs[0].pr.unresolved_threads = 5; // merged PRs must not enter To address
    prs[4].pr.unresolved_threads = 2;
    prs[7].pr.unresolved_threads = 1;
    prs[8].pr.unresolved_threads = 3;
    for (category, numbers) in [
        (Category::InitialReview, vec![13]),
        (Category::ReReview, vec![14]),
        (Category::MyChangesRequested, vec![15]),
        (Category::Merging, vec![2]),
        (Category::RecentlyMerged, vec![1]),
        (Category::Failed, vec![3, 4]),
        (Category::Pending, vec![6]),
        (Category::Approved, vec![7]),
        (Category::ToAddress, vec![8]),
        (Category::Conflicts, vec![5]),
        (Category::ChangesRequested, vec![9]),
        (Category::NeedsReview, vec![10, 12]),
        (Category::Draft, vec![11]),
    ] {
        let result = view(category, Some(prs.clone()));
        assert_eq!(result.count(), numbers.len().to_string());
        assert!(result.visible());
        assert_eq!(
            result
                .rows
                .iter()
                .map(|row| row.url.clone())
                .collect::<Vec<_>>(),
            numbers
                .iter()
                .map(|number| format!("https://example.test/repo/pull/{number}"))
                .collect::<Vec<_>>()
        );
        assert!(
            result
                .rows
                .iter()
                .all(|row| row.title.contains('<') && row.detail.contains("example/repo #"))
        );
    }
    let total: usize = Category::ALL
        .into_iter()
        .map(|category| view(category, Some(prs.clone())).rows.len())
        .sum();
    assert_eq!(total, prs.len(), "each PR must appear in exactly one pile");
}

#[test]
fn every_primary_status_has_one_bucket_with_or_without_review_threads() {
    for (status, without_threads, with_threads) in [
        (
            PrStatus::Merged,
            Category::RecentlyMerged,
            Category::RecentlyMerged,
        ),
        (PrStatus::Merging, Category::Merging, Category::Merging),
        (PrStatus::MergeFailed, Category::Failed, Category::Failed),
        (PrStatus::CiFailed, Category::Failed, Category::Failed),
        (
            PrStatus::Conflicts,
            Category::Conflicts,
            Category::Conflicts,
        ),
        (
            PrStatus::ChangesRequested,
            Category::ChangesRequested,
            Category::ChangesRequested,
        ),
        (
            PrStatus::OwnersRequired,
            Category::NeedsReview,
            Category::ToAddress,
        ),
        (
            PrStatus::NeedsReview,
            Category::NeedsReview,
            Category::ToAddress,
        ),
        (PrStatus::CiPending, Category::Pending, Category::ToAddress),
        (PrStatus::Approved, Category::Approved, Category::ToAddress),
        (PrStatus::Draft, Category::Draft, Category::ToAddress),
    ] {
        for (threads, expected) in [(0, without_threads), (1, with_threads), (3, with_threads)] {
            let mut entry = pr(3, status);
            entry.pr.unresolved_threads = threads;
            let populated = Category::ALL
                .into_iter()
                .filter(|&category| view(category, Some(vec![entry.clone()])).visible())
                .collect::<Vec<_>>();
            assert_eq!(populated, [expected], "{status:?}, threads={threads}");
        }
    }
}

#[test]
fn a_pr_moves_between_piles_without_losing_its_unseen_revision() {
    let mut entry = pr(3, PrStatus::OwnersRequired);
    for (status, threads, expected) in [
        (PrStatus::OwnersRequired, 0, Category::NeedsReview),
        (PrStatus::NeedsReview, 2, Category::ToAddress),
        (PrStatus::OwnersRequired, 0, Category::NeedsReview),
        (PrStatus::Approved, 0, Category::Approved),
        (PrStatus::CiPending, 0, Category::Pending),
        (PrStatus::CiFailed, 1, Category::Failed),
        (PrStatus::CiPending, 1, Category::ToAddress),
        (PrStatus::Approved, 0, Category::Approved),
        (PrStatus::Merging, 1, Category::Merging),
        (PrStatus::Merged, 1, Category::RecentlyMerged),
    ] {
        entry.pr.status = status;
        entry.pr.unresolved_threads = threads;
        for category in Category::ALL {
            let bucket = view(category, Some(vec![entry.clone()]));
            assert_eq!(bucket.visible(), category == expected);
            if category == expected {
                assert_eq!(bucket.count(), "1");
                assert_eq!(bucket.unseen_count(), 1);
                assert_eq!(bucket.rows[0].revision, entry.revision);
                assert_eq!(bucket.rows[0].url, entry.pr.url);
                if threads > 0 {
                    assert!(bucket.rows[0].detail.contains("unresolved review thread"));
                }
                if status == PrStatus::OwnersRequired {
                    assert!(bucket.rows[0].detail.contains("Needs review"));
                }
            } else {
                assert_eq!(bucket.count(), "0");
                assert_eq!(bucket.unseen_count(), 0);
            }
        }
    }
}

#[test]
fn absence_and_a_loaded_empty_list_are_distinct() {
    let absent = view(Category::Failed, None);
    let empty = view(Category::Failed, Some(vec![]));
    assert_eq!(absent.count(), "—");
    assert_eq!(empty.count(), "0");
    assert_ne!(absent.empty_message(), empty.empty_message());
    assert!(!absent.visible());
    for category in Category::ALL {
        assert!(
            !view(category, Some(vec![])).visible(),
            "zero-count icon was visible for {category:?}"
        );
    }
}

#[test]
fn unseen_category_marker_remains_until_each_changed_pr_is_opened() {
    let mut prs = vec![pr(3, PrStatus::CiFailed), pr(4, PrStatus::MergeFailed)];
    let before = view(Category::Failed, Some(prs.clone()));
    assert_eq!(before.unseen_count(), 2);
    assert!(before.rows.iter().all(|row| row.unseen));
    assert_eq!(
        view(Category::Failed, Some(prs.clone())),
        before,
        "reading a list is not an acknowledgment"
    );
    prs[0].unseen = false;
    let one_opened = view(Category::Failed, Some(prs.clone()));
    assert_eq!(one_opened.unseen_count(), 1);
    assert_eq!(one_opened.count(), "2");
    assert!(!one_opened.rows[0].unseen);
    assert!(one_opened.rows[1].unseen);
    prs[1].unseen = false;
    assert_eq!(view(Category::Failed, Some(prs)).unseen_count(), 0);
}
