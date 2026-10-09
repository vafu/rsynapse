use proj_model::GitStatus;
use std::{
    path::{Path, PathBuf},
    process::Command,
};
fn git(path: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let output = Command::new("git")
        // Status reads must not rewrite the watched index and trigger themselves.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .ok()?;
    output.status.success().then_some(output.stdout)
}
fn text(path: &Path, args: &[&str]) -> Option<String> {
    String::from_utf8(git(path, args)?)
        .ok()
        .map(|s| s.trim().to_owned())
}
pub fn discover(path: &Path) -> anyhow::Result<(PathBuf, PathBuf, PathBuf)> {
    let cwd = path.canonicalize()?;
    anyhow::ensure!(cwd.is_dir(), "Path must be a directory");
    let root = text(&cwd, &["rev-parse", "--show-toplevel"])
        .map(PathBuf::from)
        .unwrap_or_else(|| cwd.clone());
    let primary = text(
        &root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .map(PathBuf::from)
    .and_then(|p| p.parent().map(Path::to_path_buf))
    .unwrap_or_else(|| root.clone());
    Ok((primary, root, cwd))
}
pub fn git_dir(root: &Path) -> Option<PathBuf> {
    text(root, &["rev-parse", "--path-format=absolute", "--git-dir"]).map(PathBuf::from)
}
pub fn branch(root: &Path) -> String {
    text(root, &["branch", "--show-current"])
        .filter(|s| !s.is_empty())
        .or_else(|| text(root, &["rev-parse", "--short", "HEAD"]))
        .unwrap_or_default()
}
pub fn snapshot(root: &Path) -> (String, GitStatus) {
    let branch = branch(root);
    let mut status = GitStatus::default();
    if let Some(output) = git(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=normal"],
    ) {
        let mut records = output.split(|b| *b == 0);
        while let Some(record) = records.next() {
            if record.len() < 3 {
                continue;
            }
            let (x, y) = (record[0], record[1]);
            if x == b'?' && y == b'?' {
                status.untracked += 1;
                continue;
            }
            if x != b' ' {
                status.staged += 1;
            }
            if y != b' ' {
                status.unstaged += 1;
            }
            if matches!(x, b'R' | b'C') {
                records.next();
            }
        }
    }
    if let Some(counts) = text(
        root,
        &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
    ) {
        let mut n = counts.split_whitespace();
        status.ahead = n.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        status.behind = n.next().and_then(|n| n.parse().ok()).unwrap_or(0);
    }
    if let Some(dir) = git_dir(root) {
        status.merging = dir.join("MERGE_HEAD").exists();
        status.rebasing = dir.join("rebase-merge").exists() || dir.join("rebase-apply").exists();
    }
    status.stashes = text(root, &["stash", "list"])
        .map(|s| s.lines().count() as u32)
        .unwrap_or(0);
    (branch, status)
}
