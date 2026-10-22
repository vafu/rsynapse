use super::*;

#[test]
fn incoming_reviews_are_distinct_and_exclusive_even_when_author_side_is_blocked() {
    for status in [
        PrStatus::Merging,
        PrStatus::MergeFailed,
        PrStatus::Draft,
        PrStatus::Conflicts,
        PrStatus::ChangesRequested,
        PrStatus::OwnersRequired,
        PrStatus::CiFailed,
        PrStatus::CiPending,
        PrStatus::Approved,
        PrStatus::NeedsReview,
    ] {
        let mut item = pr(3, status);
        item.pr.review_requested = true;
        item.pr.unresolved_threads = 2;
        item.pr.checks = Some("FAILURE".into());
        for category in Category::ALL {
            let bucket = view(category, Some(vec![item.clone()]));
            if category == Category::InitialReview {
                assert_eq!(bucket.count(), "1");
                assert_eq!(bucket.unseen_count(), 1);
                assert_eq!(bucket.rows[0].revision, item.revision);
                assert_eq!(bucket.rows[0].url, item.pr.url);
                assert!(bucket.rows[0].detail.contains("Needs your initial review"));
                assert!(
                    bucket.rows[0]
                        .detail
                        .contains(if status == PrStatus::MergeFailed {
                            "Merge pipeline failed"
                        } else {
                            "CI failed"
                        })
                );
            } else {
                assert_eq!(bucket.count(), "0", "{status:?} duplicated in {category:?}");
            }
        }
        item.unseen = false;
        assert_eq!(
            view(Category::InitialReview, Some(vec![item.clone()])).count(),
            "1"
        );
        assert_eq!(
            view(Category::InitialReview, Some(vec![item.clone()])).unseen_count(),
            0
        );
        item.pr.review_requested = false;
        assert!(!view(Category::InitialReview, Some(vec![item])).visible());
    }
}

#[test]
fn completion_overrides_a_stale_review_flag_and_empty_request_bucket_is_hidden() {
    let mut item = pr(1, PrStatus::Merged);
    item.pr.review_requested = true;
    assert_eq!(Category::for_pr(&item.pr), Category::RecentlyMerged);
    assert!(!view(Category::InitialReview, Some(vec![item])).visible());
    assert!(!view(Category::InitialReview, Some(vec![])).visible());
    assert!(!view(Category::InitialReview, None).visible());
}

#[test]
fn viewer_workflow_moves_between_initial_waiting_and_rereview_not_seen_state() {
    use github::ReviewActivity;
    let mut item = pr(3, PrStatus::Approved);
    for (requested, phase, expected) in [
        (
            true,
            Some(ReviewActivity::InitialReview),
            Some(Category::InitialReview),
        ),
        (
            false,
            Some(ReviewActivity::ChangesRequested),
            Some(Category::MyChangesRequested),
        ),
        (
            true,
            Some(ReviewActivity::ReReview),
            Some(Category::ReReview),
        ),
        (false, None, None), // Approval without a fresh request is not activity.
        (
            true,
            Some(ReviewActivity::ReReview),
            Some(Category::ReReview),
        ), // Requested again after approval.
    ] {
        item.pr.review_requested = requested;
        item.pr.review_activity = phase;
        item.unseen = false; // Merely opening cannot alter the workflow bucket.
        let visible: Vec<_> = Category::ALL
            .into_iter()
            .filter(|category| category.is_activity())
            .filter(|&category| view(category, Some(vec![item.clone()])).visible())
            .collect();
        assert_eq!(visible, expected.into_iter().collect::<Vec<_>>());
        if let Some(category) = expected {
            let bucket = view(category, Some(vec![item.clone()]));
            assert_eq!(bucket.rows[0].revision, item.revision);
            assert_eq!(bucket.unseen_count(), 0);
        }
    }
}
