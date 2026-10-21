use super::*;

#[test]
fn pr_148427_failed_check_overrides_owners_approval_wait() {
    // Live 2026-10-21 D-Bus shape: the owner label wins in the service's primary
    // status, but GitHub's PR Check has failed and the raw rollup is FAILURE.
    let mut entry = pr(148427, PrStatus::OwnersRequired);
    entry.pr.repo = "Snapchat/mobile".into();
    entry.pr.url = "https://github.sc-corp.net/Snapchat/mobile/pull/148427".into();
    entry.pr.mergeable = Some("MERGEABLE".into());
    entry.pr.merge_state_status = Some("UNSTABLE".into());
    entry.unseen = true;

    for (checks, expected) in [
        ("FAILURE", Category::Failed),
        ("SUCCESS", Category::NeedsReview),
        ("ERROR", Category::Failed),
    ] {
        entry.pr.checks = Some(checks.into());
        let mut total = 0;
        for category in Category::ALL {
            let bucket = view(category, Some(vec![entry.clone()]));
            assert_eq!(bucket.visible(), category == expected);
            total += bucket.rows.len();
            if category == expected {
                assert_eq!(bucket.unseen_count(), 1);
                assert_eq!(bucket.rows[0].url, entry.pr.url);
                assert_eq!(bucket.rows[0].revision, entry.revision);
                assert_eq!(
                    bucket.rows[0].detail,
                    format!(
                        "Snapchat/mobile #148427 · {}",
                        if category == Category::Failed {
                            "CI failed"
                        } else {
                            "Needs review"
                        }
                    )
                );
            }
        }
        assert_eq!(total, 1);
    }
}

#[test]
fn failed_rollup_precedence_keeps_lifecycle_and_explicit_blockers() {
    for (status, expected) in [
        (PrStatus::Merged, Category::RecentlyMerged),
        (PrStatus::Merging, Category::Merging),
        (PrStatus::Conflicts, Category::Conflicts),
        (PrStatus::ChangesRequested, Category::ChangesRequested),
        (PrStatus::MergeFailed, Category::Failed),
        (PrStatus::CiFailed, Category::Failed),
        (PrStatus::OwnersRequired, Category::Failed),
        (PrStatus::NeedsReview, Category::Failed),
        (PrStatus::CiPending, Category::Failed),
        (PrStatus::Approved, Category::Failed),
        (PrStatus::Draft, Category::Failed),
    ] {
        for checks in ["FAILURE", "ERROR"] {
            for threads in [0, 2] {
                let mut entry = pr(3, status);
                entry.pr.checks = Some(checks.into());
                entry.pr.unresolved_threads = threads;
                let populated = Category::ALL
                    .into_iter()
                    .filter(|&category| view(category, Some(vec![entry.clone()])).visible())
                    .collect::<Vec<_>>();
                assert_eq!(
                    populated,
                    [expected],
                    "{status:?}, {checks}, threads={threads}"
                );
                if status == PrStatus::MergeFailed {
                    assert!(
                        view(expected, Some(vec![entry])).rows[0]
                            .detail
                            .contains("Merge pipeline failed")
                    );
                }
            }
        }
    }
}
