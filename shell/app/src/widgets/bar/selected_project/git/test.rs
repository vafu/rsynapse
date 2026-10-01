use std::collections::HashMap;

use super::GitStatus;

fn metadata(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn maps_steward_counts_into_status() {
    let status = GitStatus::from_metadata(&metadata(&[
        ("staged", "2"),
        ("unstaged", "1"),
        ("untracked", "3"),
        ("ahead", "2"),
        ("behind", "1"),
        ("stashes", "4"),
        ("merging", "1"),
    ]));

    assert_eq!(status.staged, 2);
    assert_eq!(status.unstaged, 1);
    assert_eq!(status.untracked, 3);
    assert_eq!(status.ahead, 2);
    assert_eq!(status.behind, 1);
    assert_eq!(status.stashes, 4);
    assert!(status.merging);
    assert!(!status.rebasing);
    assert!(status.has_changes());
}

#[test]
fn missing_keys_decode_as_clean_tree() {
    let status = GitStatus::from_metadata(&metadata(&[]));

    assert_eq!(status, GitStatus::default());
    assert!(!status.has_changes());
}

#[test]
fn ignores_unparseable_counts() {
    let status = GitStatus::from_metadata(&metadata(&[("staged", "lots"), ("ahead", "-1")]));

    assert_eq!(status.staged, 0);
    assert_eq!(status.ahead, 0);
}
