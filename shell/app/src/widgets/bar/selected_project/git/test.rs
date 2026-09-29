use std::collections::HashSet;

use super::{GitStatus, merge_state, parse_ahead_behind, parse_porcelain_status, stash_count};

#[test]
fn parses_clean_tree() {
    let status = parse_porcelain_status("# branch.head main\n");

    assert_eq!(status, GitStatus::default());
}

#[test]
fn counts_staged_unstaged_and_untracked_entries() {
    let status = parse_porcelain_status(
        "# branch.head main\n\
         1 M. N... 100644 100644 100644 abcdef file-staged.txt\n\
         1 .M N... 100644 100644 100644 abcdef file-unstaged.txt\n\
         1 AM N... 000000 100644 100644 abcdef file-both.txt\n\
         ? file-untracked.txt\n",
    );

    assert_eq!(status.staged, 2);
    assert_eq!(status.unstaged, 2);
    assert_eq!(status.untracked, 1);
}

#[test]
fn counts_unmerged_entries_on_both_sides() {
    let status =
        parse_porcelain_status("u UU N... 100644 100644 100644 abcdef file-conflict.txt\n");

    assert_eq!(status.staged, 1);
    assert_eq!(status.unstaged, 1);
}

#[test]
fn parses_ahead_behind_counts() {
    let mut status = GitStatus::default();
    parse_ahead_behind("+2 -1", &mut status);

    assert_eq!(status.ahead, 2);
    assert_eq!(status.behind, 1);
}

#[test]
fn parses_ahead_behind_from_full_status() {
    let status = parse_porcelain_status("# branch.head main\n# branch.ab +3 -0\n");

    assert_eq!(status.ahead, 3);
    assert_eq!(status.behind, 0);
}

#[test]
fn ignores_malformed_lines() {
    let status = parse_porcelain_status("1 \n2 \n# branch.ab nope\n? \n");

    assert_eq!(status, GitStatus::default());
}

#[test]
fn detects_merge_and_rebase_sentinels() {
    let dir = std::env::temp_dir().join("rsynapse-git-status-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    assert_eq!(merge_state(dir.to_str().unwrap()), (false, false));

    std::fs::write(dir.join("MERGE_HEAD"), "abc").unwrap();
    assert_eq!(merge_state(dir.to_str().unwrap()), (true, false));
    std::fs::remove_file(dir.join("MERGE_HEAD")).unwrap();

    std::fs::create_dir_all(dir.join("rebase-merge")).unwrap();
    assert_eq!(merge_state(dir.to_str().unwrap()), (false, true));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn counts_stash_entries() {
    // Outside a repository the count is zero rather than an error.
    assert_eq!(stash_count("/definitely/not/a/repo"), 0);
}

#[test]
fn reads_live_repository_status() {
    let dir = std::env::temp_dir().join("rsynapse-git-status-live-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(args)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("LC_ALL", "C")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .unwrap()
    };
    assert!(run(&["init"]).status.success());
    assert!(
        run(&["config", "user.email", "test@example.com"])
            .status
            .success()
    );
    assert!(run(&["config", "user.name", "Test"]).status.success());
    std::fs::write(dir.join("tracked.txt"), "one").unwrap();
    std::fs::write(dir.join("untracked.txt"), "new").unwrap();
    assert!(run(&["add", "tracked.txt"]).status.success());
    assert!(run(&["commit", "-m", "init"]).status.success());
    std::fs::write(dir.join("tracked.txt"), "two").unwrap();

    let status = super::read_git_status(dir.to_str().unwrap());

    let mut kinds = HashSet::new();
    if status.unstaged > 0 {
        kinds.insert("unstaged");
    }
    if status.untracked > 0 {
        kinds.insert("untracked");
    }
    assert_eq!(kinds, HashSet::from(["unstaged", "untracked"]));
    assert!(!status.merging);

    std::fs::remove_dir_all(&dir).ok();
}
