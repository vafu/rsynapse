use std::time::Duration;

use shell_core::source::{self, Observable, rx::Observable as _};

const POLL_INTERVAL: Duration = Duration::from_secs(5);

#[cfg(test)]
mod test;

/// Pure-style working-tree status for one project directory.
///
/// Counts mirror `git status --porcelain=v2` entry kinds; `merging` and
/// `rebasing` come from the git dir sentinel files, `stashes` from
/// `git stash list`. The default (all zero/false) also represents
/// non-repositories and read failures, keeping the widget invisible.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct GitStatus {
    pub(super) staged: usize,
    pub(super) unstaged: usize,
    pub(super) untracked: usize,
    pub(super) ahead: u32,
    pub(super) behind: u32,
    pub(super) merging: bool,
    pub(super) rebasing: bool,
    pub(super) stashes: usize,
}
pub(super) fn git_status(path: String) -> Observable<GitStatus> {
    source::shared_by_key("rsynapse.git-status", path.clone(), move || {
        let initial_path = path.clone();
        let poll_path = path.clone();
        // Porcelain scans take seconds on large worktrees, so every read
        // runs on the blocking pool: the initial read and the poll ticks
        // must never stall async runtime workers.
        source::from_task(move |sender| {
            let initial_path = initial_path.clone();
            let poll_path = poll_path.clone();
            async move {
                let initial =
                    tokio::task::spawn_blocking(move || read_git_status(&initial_path)).await;
                let Ok(initial) = initial else {
                    return;
                };
                if sender.send(Ok(initial)).await.is_err() {
                    return;
                }
                loop {
                    tokio::time::sleep(POLL_INTERVAL).await;
                    let tick_path = poll_path.clone();
                    let Ok(status) =
                        tokio::task::spawn_blocking(move || read_git_status(&tick_path)).await
                    else {
                        return;
                    };
                    if sender.send(Ok(status)).await.is_err() {
                        return;
                    }
                }
            }
        })
        .distinct_until_changed()
        .box_it()
    })
}

impl GitStatus {
    pub(super) fn has_changes(&self) -> bool {
        self.staged > 0
            || self.unstaged > 0
            || self.untracked > 0
            || self.ahead > 0
            || self.behind > 0
            || self.merging
            || self.rebasing
    }
}

fn read_git_status(path: &str) -> GitStatus {
    let Some(git_dir) = git_dir(path) else {
        return GitStatus::default();
    };
    let output = run_git(
        path,
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=normal",
            "--no-renames",
        ],
    );
    let mut status = output
        .as_deref()
        .map(parse_porcelain_status)
        .unwrap_or_default();
    let (merging, rebasing) = merge_state(&git_dir);
    status.merging = merging;
    status.rebasing = rebasing;
    status.stashes = stash_count(path);
    status
}

fn git_dir(path: &str) -> Option<String> {
    run_git(path, &["rev-parse", "--absolute-git-dir"])
        .map(|output| output.trim().to_owned())
        .filter(|dir| !dir.is_empty())
}

fn run_git(cwd: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C")
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_porcelain_status(output: &str) -> GitStatus {
    let mut status = GitStatus::default();
    for line in output.lines() {
        if let Some(branch) = line.strip_prefix("# branch.ab ") {
            parse_ahead_behind(branch, &mut status);
        } else if let Some(entry) = line.strip_prefix("1 ") {
            count_tracked_entry(entry, &mut status);
        } else if let Some(entry) = line.strip_prefix("2 ") {
            count_tracked_entry(entry, &mut status);
        } else if let Some(entry) = line.strip_prefix("u ") {
            count_unmerged_entry(entry, &mut status);
        } else if line.starts_with('?') && line.len() > 2 {
            status.untracked += 1;
        }
    }
    status
}

fn parse_ahead_behind(branch: &str, status: &mut GitStatus) {
    for part in branch.split_whitespace() {
        if let Some(ahead) = part.strip_prefix('+') {
            status.ahead = ahead.parse().unwrap_or(0);
        } else if let Some(behind) = part.strip_prefix('-') {
            status.behind = behind.parse().unwrap_or(0);
        }
    }
}

fn count_tracked_entry(entry: &str, status: &mut GitStatus) {
    let mut fields = entry.split_whitespace();
    let (Some(xy), Some(_subm)) = (fields.next(), fields.next()) else {
        return;
    };
    let mut flags = xy.chars();
    let (index, worktree) = (flags.next(), flags.next());
    if index.is_some_and(|flag| flag != '.') {
        status.staged += 1;
    }
    if worktree.is_some_and(|flag| flag != '.') {
        status.unstaged += 1;
    }
}

fn count_unmerged_entry(entry: &str, status: &mut GitStatus) {
    let mut fields = entry.split_whitespace();
    let (Some(xy), Some(_subm)) = (fields.next(), fields.next()) else {
        return;
    };
    let mut flags = xy.chars();
    if flags.next().is_some_and(|flag| flag == 'U') {
        status.staged += 1;
    }
    if flags.next().is_some_and(|flag| flag == 'U') {
        status.unstaged += 1;
    }
}

fn merge_state(git_dir: &str) -> (bool, bool) {
    let dir = std::path::Path::new(git_dir);
    let merging = dir.join("MERGE_HEAD").is_file();
    let rebasing = dir.join("rebase-merge").is_dir() || dir.join("rebase-apply").is_dir();
    (merging, rebasing)
}

fn stash_count(path: &str) -> usize {
    run_git(path, &["stash", "list", "--format=%gd"])
        .map(|output| {
            output
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count()
        })
        .unwrap_or(0)
}
