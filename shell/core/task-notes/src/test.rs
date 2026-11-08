use super::*;

#[test]
fn project_notebook_lives_below_the_checkout() {
    assert_eq!(
        notebook_path("/home/user/project"),
        PathBuf::from("/home/user/project/.notes")
    );
}

#[test]
fn task_tags_are_namespaced_by_immutable_instance() {
    assert_eq!(task_tag("abc123"), "task/abc123");
    assert_eq!(main_note_tag("abc123"), "task-main/abc123");
}

#[test]
fn main_note_detection_is_exact() {
    let tags = vec!["task/abc123".to_string(), "task-main/abc123".to_string()];
    assert!(is_main_note(&tags, "abc123"));
    assert!(!is_main_note(&tags, "abc124"));
}
