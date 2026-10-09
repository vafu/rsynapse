use shell_core::source::{self, Observable, rx::Observable as _};
#[cfg(test)]
mod test;
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::widgets::bar) struct GitStatus {
    pub(super) staged: usize,
    pub(super) unstaged: usize,
    pub(super) untracked: usize,
    pub(super) ahead: u32,
    pub(super) behind: u32,
    pub(super) merging: bool,
    pub(super) rebasing: bool,
    pub(super) stashes: usize,
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
    #[cfg(test)]
    fn from_metadata(metadata: &std::collections::HashMap<String, String>) -> Self {
        let count = |key: &str| {
            metadata
                .get(key)
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or_default()
        };
        Self {
            staged: count("staged") as usize,
            unstaged: count("unstaged") as usize,
            untracked: count("untracked") as usize,
            ahead: count("ahead"),
            behind: count("behind"),
            stashes: count("stashes") as usize,
            merging: metadata.contains_key("merging"),
            rebasing: metadata.contains_key("rebasing"),
        }
    }
}
pub(super) fn git_status(path: String) -> Observable<GitStatus> {
    source::proj::checkouts()
        .map(move |rows| {
            rows.into_iter()
                .find(|c| c.root_path == path)
                .map(|c| {
                    let s = c.git_status;
                    GitStatus {
                        staged: s.staged as usize,
                        unstaged: s.unstaged as usize,
                        untracked: s.untracked as usize,
                        ahead: s.ahead,
                        behind: s.behind,
                        merging: s.merging,
                        rebasing: s.rebasing,
                        stashes: s.stashes as usize,
                    }
                })
                .unwrap_or_default()
        })
        .distinct_until_changed()
        .box_it()
}

pub(super) fn summary(git: &GitStatus) -> String {
    let mut lines = vec![format!(
        "git: {} staged, {} unstaged, {} untracked",
        git.staged, git.unstaged, git.untracked
    )];
    if git.ahead > 0 || git.behind > 0 {
        lines.push(format!(
            "upstream: ahead {}, behind {}",
            git.ahead, git.behind
        ));
    }
    if git.merging {
        lines.push("merging".to_owned());
    }
    if git.rebasing {
        lines.push("rebasing".to_owned());
    }
    if git.stashes > 0 {
        lines.push(format!("stashes: {}", git.stashes));
    }
    lines.join("\n")
}
