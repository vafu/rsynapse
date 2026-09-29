use super::{ApprovalOverlay, approval_count_label, current_key, source::PendingApproval};

#[test]
fn counts_pending_approvals() {
    assert_eq!(approval_count_label(&[]), "0 pending approvals");
    assert_eq!(approval_count_label(&[approval()]), "1 pending approval");
    assert_eq!(
        approval_count_label(&[approval(), approval()]),
        "2 pending approvals"
    );
}

#[test]
fn auto_opens_once_per_request_key() {
    let mut overlay = ApprovalOverlay::new(false, None, String::new());
    overlay.approvals = vec![approval()];

    overlay.auto_open();
    assert!(overlay.visible);

    overlay.visible = false;
    overlay.auto_open();
    assert!(!overlay.visible, "same key must not reopen");
}

#[test]
fn manual_hide_pins_the_current_key() {
    let mut overlay = ApprovalOverlay::new(false, None, String::new());
    overlay.approvals = vec![approval()];
    overlay.auto_open();
    assert!(overlay.visible);

    overlay.dismissed_key = current_key(&overlay.approvals);
    overlay.visible = false;
    overlay.auto_open();
    assert!(!overlay.visible);

    let mut second = approval();
    second.request_id = "req-9".to_owned();
    second.prompt = "Other prompt".to_owned();
    overlay.approvals = vec![second];
    overlay.auto_open();
    assert!(overlay.visible, "new keys reopen");
}

#[test]
fn hides_when_requests_drain() {
    let mut overlay = ApprovalOverlay::new(
        false,
        Some("ses_1:req_2:Allow it?".to_owned()),
        String::new(),
    );
    overlay.visible = true;

    overlay.auto_open();
    assert!(!overlay.visible);
    assert_eq!(overlay.dismissed_key, None);
}

fn approval() -> PendingApproval {
    PendingApproval {
        session_path: "/io/github/AgentDBus/sessions/codex/ses_1".to_owned(),
        session_id: "ses_1".to_owned(),
        request_id: "req_2".to_owned(),
        agent_name: "codex".to_owned(),
        model_name: String::new(),
        cwd: String::new(),
        session_title: String::new(),
        prompt: "Allow it?".to_owned(),
        detail_kind: String::new(),
        detail_text: String::new(),
        options: Vec::new(),
        option_descriptions: Vec::new(),
    }
}
