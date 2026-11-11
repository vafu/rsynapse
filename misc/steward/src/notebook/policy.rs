//! Notebook location policy for project checkouts.

use std::path::{Path, PathBuf};

use goal_model::ProjectInfo;

/// Resolve the notebook for a focused project.
///
/// `None` means the remarked default notebook. A focused project without a
/// project-local `.notes` directory also falls back to the default rather than
/// leaving a previous project's notebook selected.
pub(crate) fn notebook_scope(project: Option<&ProjectInfo>) -> Option<PathBuf> {
    let project = project?;
    let candidate = Path::new(&project.cwd).join(".notes");
    candidate.is_dir().then_some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(cwd: impl Into<String>) -> ProjectInfo {
        ProjectInfo {
            id: String::new(),
            name: String::new(),
            cwd: cwd.into(),
            checkout_id: String::new(),
            icon: String::new(),
            icon_origin: String::new(),
        }
    }

    #[test]
    fn no_focused_project_uses_default() {
        assert_eq!(notebook_scope(None), None);
    }

    #[test]
    fn missing_project_notebook_uses_default() {
        let project = project("/definitely/not/a/real/project");
        assert_eq!(notebook_scope(Some(&project)), None);
    }

    #[test]
    fn project_local_notes_directory_is_selected() {
        let root = std::env::temp_dir().join(format!("steward-notebook-{}", std::process::id()));
        let notes = root.join(".notes");
        std::fs::create_dir_all(&notes).unwrap();
        assert_eq!(notebook_scope(Some(&project(root.display().to_string()))), Some(notes));
        std::fs::remove_dir_all(root).unwrap();
    }
}
