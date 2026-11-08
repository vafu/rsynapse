//! Shell-owned convention joining projd task instances to Remarked notes.
//!
//! Projd remains authoritative for task identity and Remarked remains
//! authoritative for note storage. This crate only names the shared convention:
//! a project notebook lives at `<project-cwd>/.notes`, and notes associate with
//! a projd task through the immutable `task/<instance-id>` frontmatter tag.
//!
//! Neither `projd` nor Remarked depends on this crate.
use std::path::{Path, PathBuf};

pub const NOTEBOOK_DIR: &str = ".notes";

/// Notebook directory for a registered project checkout.
pub fn notebook_path(project_cwd: impl AsRef<Path>) -> PathBuf {
    project_cwd.as_ref().join(NOTEBOOK_DIR)
}

/// Frontmatter tag associating a note with an immutable projd task instance.
pub fn task_tag(instance_id: &str) -> String {
    format!("task/{instance_id}")
}

/// Marks the shell-seeded primary note for a task instance.
pub fn main_note_tag(instance_id: &str) -> String {
    format!("task-main/{instance_id}")
}

/// Returns whether tags contain the primary-note marker for a task instance.
pub fn is_main_note(tags: &[String], instance_id: &str) -> bool {
    let expected = main_note_tag(instance_id);
    tags.iter().any(|tag| tag == &expected)
}

#[cfg(test)]
mod test;
