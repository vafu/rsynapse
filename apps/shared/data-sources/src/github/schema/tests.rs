use super::*;
use serde_json::json;

#[test]
fn decodes_current_activity_payload_and_preserves_review_details() {
    let rows: Vec<PullRequestActivity> = serde_json::from_value(json!([{
        "pr": {
            "title": "Update a dependency", "url": "https://example.test/team/repo/pull/7",
            "number": 7, "repo": "team/repo", "status": "owners_required",
            "cool_status": "Validating", "merged_at": null,
            "review_decision": "REVIEW_REQUIRED", "mergeable": "MERGEABLE",
            "merge_state_status": "BLOCKED", "checks": "SUCCESS",
            "updated_at": "2026-10-30T12:00:00Z", "unresolved_threads": 2,
            "latest_review": {"state": "APPROVED", "author": "reviewer",
                              "submitted_at": "2026-10-30T11:00:00Z"},
            "labels": ["team-a"], "review_requested": true,
            "review_activity": "re_review", "future_metadata": {"enabled": true}
        },
        "revision": "opaque:revision/7", "unseen": true, "future_flag": true
    }]))
    .unwrap();
    let row = &rows[0];
    assert_eq!(row.pr.status, PrStatus::OwnersRequired);
    assert_eq!(row.pr.review_activity, Some(ReviewActivity::ReReview));
    assert!(row.pr.review_requested);
    assert_eq!(row.pr.latest_review.as_ref().unwrap().author, "reviewer");
    assert_eq!(row.pr.cool_status.as_deref(), Some("Validating"));
    assert_eq!(row.pr.unresolved_threads, 2);
    assert_eq!(row.pr.labels, ["team-a"]);
    assert_eq!(row.revision, "opaque:revision/7");
    assert!(row.unseen);
}

#[test]
fn older_payloads_default_only_optional_pr_fields_and_rollup_counters() {
    let pr: PullRequest = serde_json::from_str(
        r#"{
        "title":"Legacy PR", "url":"https://example.test/repo/pull/1",
        "number":1, "repo":"team/repo", "status":"needs_review", "updated_at":""
    }"#,
    )
    .unwrap();
    assert!(pr.labels.is_empty());
    assert_eq!(pr.unresolved_threads, 0);
    assert!(!pr.review_requested);
    assert_eq!(pr.review_activity, None);
    assert_eq!(pr.latest_review, None);
    assert_eq!(pr.cool_status, None);

    let overview: PrOverview =
        serde_json::from_str(r#"{"open":3,"merging":1,"future_count":4}"#).unwrap();
    assert_eq!(overview.open, 3);
    assert_eq!(overview.merging, 1);
    assert_eq!(overview.re_review, 0);
    assert_eq!(overview.to_address, 0);
    // Missing revision/unseen must not invent an acknowledgment state.
    assert!(serde_json::from_value::<PullRequestActivity>(json!({"pr": pr})).is_err());
}

#[test]
fn status_spellings_and_flat_summary_match_the_wire_contract() {
    for (wire, status) in [
        ("merged", PrStatus::Merged),
        ("merging", PrStatus::Merging),
        ("merge_failed", PrStatus::MergeFailed),
        ("draft", PrStatus::Draft),
        ("conflicts", PrStatus::Conflicts),
        ("changes_requested", PrStatus::ChangesRequested),
        ("owners_required", PrStatus::OwnersRequired),
        ("ci_failed", PrStatus::CiFailed),
        ("ci_pending", PrStatus::CiPending),
        ("approved", PrStatus::Approved),
        ("needs_review", PrStatus::NeedsReview),
    ] {
        assert_eq!(
            serde_json::from_value::<PrStatus>(json!(wire)).unwrap(),
            status
        );
        assert_eq!(serde_json::to_value(status).unwrap(), json!(wire));
    }
    for (wire, activity) in [
        ("initial_review", ReviewActivity::InitialReview),
        ("re_review", ReviewActivity::ReReview),
        ("changes_requested", ReviewActivity::ChangesRequested),
    ] {
        assert_eq!(
            serde_json::from_value::<ReviewActivity>(json!(wire)).unwrap(),
            activity
        );
        assert_eq!(serde_json::to_value(activity).unwrap(), json!(wire));
    }
    let payload = json!({"ci_pending": 2, "approved": 1, "total": 3});
    let summary: PrSummary = serde_json::from_value(payload.clone()).unwrap();
    assert_eq!(summary.counts[&PrStatus::CiPending], 2);
    assert_eq!(summary.counts[&PrStatus::Approved], 1);
    assert_eq!(summary.total, 3);
    assert_eq!(serde_json::to_value(summary).unwrap(), payload);
}

#[test]
fn notification_signal_accepts_open_reason_and_kind_values() {
    let event: NotificationEvent = serde_json::from_str(
        r#"{
        "thread_id":"thread-7", "reason":"new_reason", "kind":"NewSubjectKind",
        "title":"Notification", "repo":"team/repo", "updated_at":"", "extra":true
    }"#,
    )
    .unwrap();
    assert_eq!(event.thread_id, "thread-7");
    assert_eq!(event.reason, "new_reason");
    assert_eq!(event.kind, "NewSubjectKind");
    assert_eq!(event.url, None);
}
