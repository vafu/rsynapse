use super::*;
use crate::tasknotes::frontmatter::split;
use serde_yaml::Mapping;

fn mapping(tags: &[&str]) -> Mapping {
    let mut mapping = Mapping::new();
    frontmatter::set_tags(
        &mut mapping,
        tags.iter().map(|tag| tag.to_string()).collect(),
    );
    mapping
}

#[test]
fn valid_task_note_has_no_findings() {
    let mapping = mapping(&["task/abc", "note/decision", "kind/decision"]);
    let report = check(&mapping, "abc");
    assert!(report.is_clean(), "{:?}", report.findings);
}

#[test]
fn missing_task_tag_is_an_error() {
    let mapping = mapping(&["note/decision", "kind/decision"]);
    let report = check(&mapping, "abc");
    assert_eq!(report.errors(), 1);
    assert!(report.findings[0].message.contains("task/abc"));
}

#[test]
fn wrong_instance_is_an_error() {
    let mapping = mapping(&["task/other", "note/decision", "kind/decision"]);
    assert_eq!(check(&mapping, "abc").errors(), 1);
}

#[test]
fn zero_or_many_kinds_are_errors() {
    let none = mapping(&["task/abc", "note/decision"]);
    assert_eq!(check(&none, "abc").errors(), 1);
    let many = mapping(&["task/abc", "note/decision", "kind/decision", "kind/idea"]);
    assert_eq!(check(&many, "abc").errors(), 1);
}

#[test]
fn missing_role_is_an_error() {
    let mapping = mapping(&["task/abc", "kind/decision"]);
    assert_eq!(check(&mapping, "abc").errors(), 1);
}

#[test]
fn inbox_is_a_warning() {
    let mapping = mapping(&["task/abc", "note/progress", "kind/fleeting", "status/inbox"]);
    let report = check(&mapping, "abc");
    assert_eq!(report.errors(), 0);
    assert_eq!(report.warnings(), 1);
}

#[test]
fn fix_adds_task_role_kind_and_drops_inbox() {
    let mut mapping = mapping(&["kind/fleeting", "status/inbox", "topic/x"]);
    fix(&mut mapping, "abc", None).unwrap();
    let report = check(&mapping, "abc");
    assert!(report.is_clean(), "{:?}", report.findings);
    let tags = frontmatter::tags(&mapping);
    assert!(tags.contains(&"task/abc".to_string()));
    assert!(tags.contains(&"note/main".to_string()));
    assert!(tags.contains(&"kind/fleeting".to_string()));
    assert!(tags.contains(&"topic/x".to_string()));
    assert!(!tags.contains(&"status/inbox".to_string()));
}

#[test]
fn fix_preserves_existing_role() {
    let mut mapping = mapping(&["task/abc", "note/decision", "kind/decision"]);
    fix(&mut mapping, "abc", Some("progress")).unwrap();
    assert_eq!(role_of(&mapping).as_deref(), Some("decision"));
}

#[test]
fn parse_accepts_plain_namespaced_or_bare_roles() {
    assert_eq!(parse_role("decision").unwrap(), "decision");
    assert_eq!(parse_role("note/decision").unwrap(), "decision");
    assert!(parse_role(" ").is_err());
    assert!(parse_role("two words").is_err());
}

#[test]
fn new_note_mapping_is_canonical() {
    let new = NewNote {
        instance_id: "abc".into(),
        role: "main".into(),
        kind: Some("task".into()),
        title: Some("My task".into()),
        extra_tags: vec!["topic/x".into()],
    };
    let mapping = new.mapping();
    assert!(check(&mapping, "abc").is_clean());
    assert_eq!(frontmatter::title(&mapping).as_deref(), Some("My task"));
}

#[test]
fn split_requires_well_formed_fence() {
    let document = split("---\ntags:\n  - task/abc\n---\nbody\n");
    assert!(document.yaml.is_some());
    assert_eq!(document.body, "body\n");
    assert!(split("no frontmatter").yaml.is_none());
    assert!(split("---\nunterminated\nbody").yaml.is_none());
}

#[test]
fn broken_yaml_fails_to_parse() {
    let document = split("---\ntags: [task/abc\n---\nbody\n");
    assert!(parse(&document).is_err());
}
