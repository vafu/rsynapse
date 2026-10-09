use crate::Manager;
use notify::Watcher;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};
use tokio::sync::{mpsc, oneshot};
use zbus::Connection;

pub(super) type Request = (String, PathBuf, bool, oneshot::Sender<()>);

fn matches(dir: &Path, path: &Path) -> bool {
    if path.starts_with(dir.join("refs")) {
        return true;
    }
    path.parent() == Some(dir)
        && path.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
            matches!(
                n,
                "HEAD"
                    | "index"
                    | "packed-refs"
                    | "FETCH_HEAD"
                    | "ORIG_HEAD"
                    | "MERGE_HEAD"
                    | "rebase-merge"
                    | "rebase-apply"
            )
        })
}

fn refresh(
    manager: &Manager,
    conn: &Connection,
    root: String,
    done: &mpsc::UnboundedSender<String>,
) {
    let manager = manager.clone();
    let conn = conn.clone();
    let done = done.clone();
    tokio::spawn(async move {
        if let Err(error) = manager.refresh_checkout(root.clone(), &conn).await {
            eprintln!("[projd/git-refresh] {root}: {error}");
        }
        let _ = done.send(root);
    });
}

pub(super) fn start(
    manager: Manager,
    conn: Connection,
    mut requests: mpsc::UnboundedReceiver<Request>,
) -> anyhow::Result<()> {
    let (tx, mut events) = mpsc::unbounded_channel();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event {
            if !matches!(event.kind, notify::EventKind::Access(_)) {
                let _ = tx.send(event.paths);
            }
        }
    })?;
    tokio::spawn(async move {
        let (done, mut completed) = mpsc::unbounded_channel();
        let mut watched = HashSet::new();
        let mut directories = HashMap::<String, PathBuf>::new();
        let mut running = HashSet::<String>::new();
        let mut dirty = HashSet::<String>::new();
        loop {
            let roots = tokio::select! {
                request=requests.recv()=>{
                    let Some((root,dir,initial,ready))=request else{break;};
                    if watched.insert(dir.clone()) {
                        let _=watcher.watch(&dir,notify::RecursiveMode::NonRecursive);
                        if dir.join("refs").exists() { let _=watcher.watch(&dir.join("refs"),notify::RecursiveMode::Recursive); }
                    }
                    directories.insert(root.clone(),dir);
                    let _=ready.send(());
                    if initial { vec![root] } else { Vec::new() }
                },
                paths=events.recv()=>{
                    let Some(paths)=paths else{break;};
                    directories.iter().filter(|(_,dir)|paths.iter().any(|p|matches(dir,p))).map(|(root,_)|root.clone()).collect()
                },
                root=completed.recv()=>{
                    let Some(root)=root else{break;};
                    running.remove(&root);
                    if dirty.remove(&root) { vec![root] } else { Vec::new() }
                },
            };
            for root in roots {
                if running.insert(root.clone()) {
                    refresh(&manager, &conn, root, &done);
                } else {
                    dirty.insert(root);
                }
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worktree_changes_and_locks_do_not_refresh_the_primary_checkout() {
        let primary = Path::new("/repo/.git");
        let worktree = primary.join("worktrees/feature");
        assert!(matches(primary, &primary.join("index")));
        assert!(matches(primary, &primary.join("refs/heads/main")));
        assert!(!matches(primary, &primary.join("index.lock")));
        assert!(!matches(primary, &worktree.join("index")));
        assert!(matches(&worktree, &worktree.join("HEAD")));
        assert!(!matches(&worktree, &worktree.join("HEAD.lock")));
    }
}
