use super::{
    LegacyPending, PendingArrays, SessionMeta, approval_key, approvals_for_session, pretty_path,
    with_default_options,
};

const PATH: &str = "/io/github/AgentDBus/sessions/codex/ses_1";

#[test]
fn shortens_home_directories() {
    assert_eq!(pretty_path("/home/v47/proj/rsynapse"), "~/proj/rsynapse");
    assert_eq!(pretty_path("/tmp/work"), "/tmp/work");
    assert_eq!(pretty_path(""), "unknown cwd");
}

#[test]
fn builds_stable_approval_keys() {
    let approval = &approvals_for_session(PATH, meta(), arrays(), legacy())[0];
    assert_eq!(approval_key(approval), "ses_1:req_2:Allow it?");
}

#[test]
fn defaults_empty_options_to_allow_deny() {
    assert_eq!(
        with_default_options(Vec::new()),
        vec!["Allow".to_owned(), "Deny".to_owned()]
    );
    assert_eq!(
        with_default_options(vec!["Once".to_owned()]),
        vec!["Once".to_owned()]
    );
}

#[test]
fn maps_indexed_pending_requests() {
    let approvals = approvals_for_session(PATH, meta(), arrays(), legacy());

    assert_eq!(approvals.len(), 1);
    let approval = &approvals[0];
    assert_eq!(
        approval.session_path,
        "/io/github/AgentDBus/sessions/codex/ses_1"
    );
    assert_eq!(approval.session_id, "ses_1");
    assert_eq!(approval.request_id, "req_2");
    assert_eq!(approval.agent_name, "codex");
    assert_eq!(approval.model_name, "codex-mini");
    assert_eq!(approval.cwd, "/home/v47/proj");
    assert_eq!(approval.prompt, "Allow it?");
    assert_eq!(approval.detail_kind, "command");
    assert_eq!(approval.detail_text, "cargo test --workspace");
    assert_eq!(
        approval.options,
        vec!["Allow".to_owned(), "Deny".to_owned()]
    );
    assert_eq!(
        approval.option_descriptions,
        vec!["Yes".to_owned(), String::new()]
    );
}

#[test]
fn falls_back_to_legacy_singular_properties() {
    let arrays = PendingArrays {
        requires_attention: true,
        ..PendingArrays::default()
    };
    let legacy = LegacyPending {
        prompt: "Legacy prompt".to_owned(),
        ..LegacyPending::default()
    };

    let approvals = approvals_for_session(PATH, meta(), arrays, legacy);

    assert_eq!(approvals.len(), 1);
    assert_eq!(approvals[0].prompt, "Legacy prompt");
    assert_eq!(
        approvals[0].options,
        vec!["Allow".to_owned(), "Deny".to_owned()]
    );
    assert_eq!(approvals[0].request_id, "");
}

#[test]
fn skips_sessions_without_attention_or_prompts() {
    let idle = PendingArrays {
        requires_attention: false,
        ..arrays()
    };
    assert!(approvals_for_session(PATH, meta(), idle, legacy()).is_empty());

    let attention_without_prompt = PendingArrays {
        request_ids: vec!["req_2".to_owned()],
        prompts: Vec::new(),
        ..arrays()
    };
    let empty_legacy = LegacyPending::default();
    assert!(approvals_for_session(PATH, meta(), attention_without_prompt, empty_legacy).is_empty());

    assert!(approvals_for_session(PATH, meta(), PendingArrays::default(), legacy()).is_empty());
}

fn meta() -> SessionMeta {
    SessionMeta {
        session_id: "ses_1".to_owned(),
        agent_name: "codex".to_owned(),
        model_name: "codex-mini".to_owned(),
        cwd: "/home/v47/proj".to_owned(),
        session_title: String::new(),
    }
}

fn arrays() -> PendingArrays {
    PendingArrays {
        requires_attention: true,
        request_ids: vec!["req_2".to_owned()],
        prompts: vec!["Allow it?".to_owned()],
        detail_kinds: vec!["command".to_owned()],
        detail_texts: vec!["cargo test --workspace".to_owned()],
        options_list: vec![vec!["Allow".to_owned(), "Deny".to_owned()]],
        descriptions_list: vec![vec!["Yes".to_owned(), String::new()]],
    }
}

fn legacy() -> LegacyPending {
    LegacyPending::default()
}
