use super::actual::agent_icon;

#[test]
fn maps_agent_name_to_renderable_icon() {
    assert_eq!(agent_icon("codex", "", ""), "cognition");
    assert_eq!(agent_icon(" Codex ", "ignored", "ignored"), "cognition");
}

#[test]
fn ignores_arbitrary_metadata_as_icon_names() {
    assert_eq!(
        agent_icon("unknown-agent", "not an icon", "reviewer"),
        "cognition"
    );
    assert_eq!(agent_icon("", "", ""), "cognition");
}
