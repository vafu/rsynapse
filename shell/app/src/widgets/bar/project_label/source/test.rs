use super::agent::workspace_agent_state_from_agents;
use super::build::{WorkspaceBuildState, workspace_build_state_from_builds};
use super::workspace_icon::{
    fallback_icon_for_context, parse_pick_icon_output, picker_cache_key_for_context,
    picker_input_for_context, picker_strings_for_context, workspace_icon_context_from_parts,
};
use crate::widgets::bar::window_tile::agent::{Agent, State};
use crate::widgets::bar::{bzbus::BzBusView, project::ProjectDetails};

#[test]
fn workspace_agent_state_tracks_unseen_agents() {
    let state = workspace_agent_state_from_agents(vec![Some(agent(State::Idle, false, true))]);

    assert!(state.has_unseen);
    assert!(!state.has_working);
    assert!(!state.has_attention);
}

#[test]
fn workspace_agent_state_keeps_working_and_attention_precedence() {
    let state = workspace_agent_state_from_agents(vec![
        Some(agent(State::Idle, false, true)),
        Some(agent(State::ToolUse, false, false)),
        Some(agent(State::None, true, false)),
    ]);

    assert!(state.has_unseen);
    assert!(state.has_working);
    assert!(state.has_attention);
}

#[test]
fn workspace_build_state_uses_failed_running_finished_precedence() {
    assert_eq!(
        workspace_build_state_from_builds(vec![Some(build("running")), Some(build("failed"))]),
        WorkspaceBuildState::Failed
    );
    assert_eq!(
        workspace_build_state_from_builds(vec![Some(build("finished")), Some(build("running"))]),
        WorkspaceBuildState::Running
    );
    assert_eq!(
        workspace_build_state_from_builds(vec![Some(build("finished")), Some(build("finished"))]),
        WorkspaceBuildState::Finished
    );
    assert_eq!(
        workspace_build_state_from_builds(vec![Some(build("idle"))]),
        WorkspaceBuildState::None
    );
}

#[test]
fn workspace_icon_uses_project_icon_before_context_hints() {
    let context = workspace_icon_context_from_parts(
        ProjectDetails {
            has_project: true,
            icon: Some("account_tree".to_owned()),
            cwd_label: Some(".config/rsynapse".to_owned()),
            ..ProjectDetails::default()
        },
        vec!["slack".to_owned(), "com.mitchellh.ghostty".to_owned()],
    );

    assert_eq!(fallback_icon_for_context(&context), "account_tree");
}

#[test]
fn workspace_icon_feeds_raw_context_to_picker() {
    let context = workspace_icon_context_from_parts(
        ProjectDetails {
            display_main: Some("rsynapse".to_owned()),
            cwd_label: Some(".config/rsynapse".to_owned()),
            branch: Some("main".to_owned()),
            ..ProjectDetails::default()
        },
        vec!["slack".to_owned(), "google-chrome".to_owned()],
    );

    assert_eq!(fallback_icon_for_context(&context), "workspaces");
    assert_eq!(
        picker_strings_for_context(&context),
        ["rsynapse", ".config/rsynapse", "main"]
    );
    assert_eq!(
        picker_input_for_context(&context),
        "rsynapse\n.config/rsynapse\nmain"
    );
}

#[test]
fn workspace_icon_uses_sorted_app_context_without_project_context() {
    let context = workspace_icon_context_from_parts(
        ProjectDetails::default(),
        vec![
            "slack".to_owned(),
            "google-chrome".to_owned(),
            "slack".to_owned(),
        ],
    );

    assert_eq!(
        picker_strings_for_context(&context),
        ["google-chrome", "slack"]
    );
}

#[test]
fn workspace_icon_falls_back_to_workspace_symbol() {
    let context = workspace_icon_context_from_parts(ProjectDetails::default(), Vec::new());

    assert_eq!(fallback_icon_for_context(&context), "workspaces");
}

#[test]
fn workspace_icon_skips_low_signal_picker_context() {
    let context = workspace_icon_context_from_parts(
        ProjectDetails::default(),
        vec!["com.mitchellh.ghostty".to_owned()],
    );

    assert_eq!(picker_cache_key_for_context(&context), None);
}

#[test]
fn workspace_icon_parses_pick_icon_json() {
    assert_eq!(
        parse_pick_icon_output(br#"[{"icon":"communication","score":1.0}]"#),
        Some("communication".to_owned())
    );
    assert_eq!(
        parse_pick_icon_output(br#"[{"icon":"phishing","score":0.6931895017623901}]"#),
        None
    );
    assert_eq!(parse_pick_icon_output(b"[]"), None);
}

fn agent(state: State, attention: bool, unseen: bool) -> Agent {
    Agent {
        name: "codex".to_owned(),
        icon: "cognition".to_owned(),
        cwd: String::new(),
        title: String::new(),
        attention,
        state,
        unseen,
    }
}

fn build(state: &'static str) -> BzBusView {
    BzBusView {
        classes: vec!["bar-item", "bzbus-widget", state],
        tooltip: String::new(),
        icon: "",
        progress_level_classes: vec![],
        progress_percent: 0,
        progress_visible: false,
    }
}
