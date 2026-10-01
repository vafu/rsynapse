use std::{collections::HashMap, time::Duration};

use crate::relations::LocusClient;

const POLL_INTERVAL: Duration = Duration::from_secs(5);

pub struct GitStatus {
    locus: LocusClient,
}

impl GitStatus {
    pub fn new(locus: LocusClient) -> Self {
        Self { locus }
    }

    /// Poll every project currently mapped to a workspace and publish
    /// changed snapshots as `org.rsynapse.project.git-status` relations.
    /// Unchanged snapshots are not rewritten, so subscribers only wake on
    /// real changes. All git work runs on the blocking pool.
    pub async fn run(self) -> anyhow::Result<()> {
        let mut last_sent: HashMap<String, Snapshot> = HashMap::new();
        let mut tick = tokio::time::interval(POLL_INTERVAL);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            let mut paths = project_paths(&self.locus).await;
            paths.sort();
            paths.dedup();
            for path in &paths {
                let owned = path.clone();
                let snapshot = tokio::task::spawn_blocking(move || read_git_status(&owned))
                    .await
                    .unwrap_or_default();
                if last_sent.get(path) != Some(&snapshot) {
                    self.publish(path, &snapshot).await;
                    last_sent.insert(path.clone(), snapshot);
                }
            }
            last_sent.retain(|path, _| paths.contains(path));
        }
    }

    async fn publish(&self, path: &str, snapshot: &Snapshot) {
        let subject =
            locus::RelationEndpoint::stable_key(locus::keys::PROJECT_PATH, path.to_owned());
        if let Err(error) = self
            .locus
            .set_one(
                subject.clone(),
                crate::relations::PROJECT_GIT_STATUS,
                subject,
                snapshot.metadata(),
            )
            .await
        {
            eprintln!("[rsynapse-steward/git-status] publish failed: {error}");
        }
    }
}

async fn project_paths(locus: &LocusClient) -> Vec<String> {
    let Ok(records) = locus.list(crate::relations::WORKSPACE_PROJECT).await else {
        return Vec::new();
    };
    records
        .into_iter()
        .filter_map(|record| {
            project_path(&record.target)
                .or_else(|| metadata_value(&record.metadata, &["path"]))
        })
        .collect()
}

fn project_path(endpoint: &locus::RelationEndpoint) -> Option<String> {
    match endpoint {
        locus::RelationEndpoint::StableKey { kind, id }
            if kind == locus::keys::PROJECT_PATH =>
        {
            non_empty(id.clone())
        }
        _ => None,
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Snapshot {
    staged: usize,
    unstaged: usize,
    untracked: usize,
    ahead: u32,
    behind: u32,
    merging: bool,
    rebasing: bool,
    stashes: usize,
}

impl Snapshot {
    fn metadata(&self) -> HashMap<String, String> {
        let mut metadata = HashMap::from([
            ("staged".to_owned(), self.staged.to_string()),
            ("unstaged".to_owned(), self.unstaged.to_string()),
            ("untracked".to_owned(), self.untracked.to_string()),
            ("ahead".to_owned(), self.ahead.to_string()),
            ("behind".to_owned(), self.behind.to_string()),
            ("stashes".to_owned(), self.stashes.to_string()),
        ]);
        if self.merging {
            metadata.insert("merging".to_owned(), "1".to_owned());
        }
        if self.rebasing {
            metadata.insert("rebasing".to_owned(), "1".to_owned());
        }
        metadata
    }
}

fn read_git_status(path: &str) -> Snapshot {
    let Some(git_dir) = git_dir(path) else {
        return Snapshot::default();
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

fn parse_porcelain_status(output: &str) -> Snapshot {
    let mut status = Snapshot::default();
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

fn parse_ahead_behind(branch: &str, status: &mut Snapshot) {
    for part in branch.split_whitespace() {
        if let Some(ahead) = part.strip_prefix('+') {
            status.ahead = ahead.parse().unwrap_or(0);
        } else if let Some(behind) = part.strip_prefix('-') {
            status.behind = behind.parse().unwrap_or(0);
        }
    }
}

fn count_tracked_entry(entry: &str, status: &mut Snapshot) {
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

fn count_unmerged_entry(entry: &str, status: &mut Snapshot) {
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

fn metadata_value(metadata: &HashMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| metadata.get(*key).cloned())
        .and_then(|value| {
            let value = value.trim().to_owned();
            (!value.is_empty()).then_some(value)
        })
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clean_tree() {
        let status = parse_porcelain_status("# branch.head main\n");

        assert_eq!(status, Snapshot::default());
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
        let mut status = Snapshot::default();
        parse_ahead_behind("+2 -1", &mut status);

        assert_eq!(status.ahead, 2);
        assert_eq!(status.behind, 1);
    }

    #[test]
    fn unchanged_snapshots_serialize_stably() {
        let status = Snapshot {
            staged: 1,
            merging: true,
            ..Snapshot::default()
        };
        let metadata = status.metadata();

        assert_eq!(metadata.get("staged").map(String::as_str), Some("1"));
        assert_eq!(metadata.get("merging").map(String::as_str), Some("1"));
        assert!(!metadata.contains_key("rebasing"));
    }
}
