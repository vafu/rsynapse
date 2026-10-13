use proj_model::GoalInfo;
use std::{io::Write, path::PathBuf};

fn directory() -> PathBuf {
    if let Some(path) = std::env::var_os("PROJD_GOALS_DIR") {
        return path.into();
    }
    if let Some(path) = std::env::var_os("PROJD_STORE_PATH") {
        return PathBuf::from(path).parent().unwrap().join("goals");
    }
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap()).join(".local/state"))
        .join("proj/goals")
}

pub fn ensure(goal: &mut GoalInfo) -> anyhow::Result<()> {
    let dir = if goal.date.is_empty() {
        directory()
    } else {
        directory().join("dated").join(&goal.date)
    };
    std::fs::create_dir_all(&dir)?;
    let dir = dir.canonicalize()?;
    let path = dir.join(format!("{}.md", goal.id));
    if goal.note_path.is_empty() {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                write!(file, "# {}\n\n{}\n", goal.title, goal.description)?;
                if !goal.success.is_empty() {
                    write!(file, "\n## Success criterion\n\n{}\n", goal.success)?;
                }
                write!(file, "\n## Notes\n\n## Log\n")?;
                file.sync_all()?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        goal.note_path = path.to_string_lossy().into_owned();
    }
    Ok(())
}
