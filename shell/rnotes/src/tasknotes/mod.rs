//! Task-note conventions owned by `rnotes`.
//!
//! A task note lives under `tasks/`, carries `task/<instance-id>` so Remarked
//! scoping finds it, one `note/<role>` role tag, and exactly one `kind/*` tag.
//! Task notes are never `status/inbox`: they belong to a task, not to global
//! triage.
pub mod frontmatter;

use std::path::{Path, PathBuf};

use serde_yaml::Mapping;

pub const TASKS_DIR: &str = "tasks";
pub const DEFAULT_ROLE: &str = "main";
pub const MAIN_ROLE: &str = "main";
pub const DEFAULT_KIND: &str = "fleeting";
pub const DEFAULT_TITLE: &str = "Note";

const ROLE_NAMESPACE: &str = "note";
const KIND_NAMESPACE: &str = "kind";
const INBOX_TAG: &str = "status/inbox";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    pub message: String,
}

impl Finding {
    fn error(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
        }
    }

    fn warning(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            message: message.into(),
        }
    }
}

#[derive(Debug, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
}

impl Report {
    pub fn errors(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == Severity::Error)
            .count()
    }

    pub fn warnings(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == Severity::Warning)
            .count()
    }

    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

/// Validates a parsed note against the task-note contract.
pub fn check(mapping: &Mapping, instance_id: &str) -> Report {
    let mut report = Report::default();
    let tags = frontmatter::tags(mapping);

    if !tags
        .iter()
        .any(|tag| tag == &task_notes::task_tag(instance_id))
    {
        report.findings.push(Finding::error(format!(
            "missing task tag {}",
            task_notes::task_tag(instance_id)
        )));
    }

    let kinds = frontmatter::tags_with_prefix(mapping, KIND_NAMESPACE);
    match kinds.len() {
        0 => report.findings.push(Finding::error("missing kind/* tag")),
        1 => {}
        _ => report.findings.push(Finding::error(format!(
            "expected exactly one kind/* tag, found {}: {}",
            kinds.len(),
            kinds.join(", ")
        ))),
    }

    let roles = frontmatter::tags_with_prefix(mapping, ROLE_NAMESPACE);
    if roles.is_empty() {
        report
            .findings
            .push(Finding::error("missing note/* role tag"));
    }

    if tags.iter().any(|tag| tag == INBOX_TAG) {
        report
            .findings
            .push(Finding::warning("task note still carries status/inbox"));
    }

    report
}

/// Applies safe repairs. Returns `Err` when frontmatter does not parse, since
/// rewriting unparseable YAML would risk dropping user metadata.
pub fn fix(mapping: &mut Mapping, instance_id: &str, role: Option<&str>) -> Result<(), String> {
    let mut tags = frontmatter::tags(mapping);
    tags.retain(|tag| tag != INBOX_TAG);
    frontmatter::set_tags(mapping, tags);

    if frontmatter::tags_with_prefix(mapping, KIND_NAMESPACE).is_empty() {
        frontmatter::replace_tag_namespace(
            mapping,
            KIND_NAMESPACE,
            &[format!("{KIND_NAMESPACE}/{DEFAULT_KIND}")],
        );
    }

    let task_tag = task_notes::task_tag(instance_id);
    let mut tags = frontmatter::tags(mapping);
    if !tags.contains(&task_tag) {
        tags.push(task_tag);
    }
    if frontmatter::tags_with_prefix(mapping, ROLE_NAMESPACE).is_empty() {
        let role = role.unwrap_or(DEFAULT_ROLE);
        tags.push(format!("{ROLE_NAMESPACE}/{role}"));
    }
    frontmatter::set_tags(mapping, tags);
    Ok(())
}

/// Canonical frontmatter for a freshly created task note.
pub struct NewNote {
    pub instance_id: String,
    pub role: String,
    pub kind: Option<String>,
    pub title: Option<String>,
    pub extra_tags: Vec<String>,
}

impl NewNote {
    pub fn mapping(&self) -> Mapping {
        let mut mapping = Mapping::new();
        let mut tags = vec![
            task_notes::task_tag(&self.instance_id),
            format!("{ROLE_NAMESPACE}/{}", self.role),
            format!(
                "{KIND_NAMESPACE}/{}",
                self.kind.as_deref().unwrap_or(DEFAULT_KIND)
            ),
        ];
        if is_main_role(&self.role) {
            tags.push(task_notes::main_note_tag(&self.instance_id));
        }
        tags.extend(self.extra_tags.iter().cloned());
        frontmatter::set_tags(&mut mapping, tags);
        frontmatter::set_title(&mut mapping, self.title.as_deref().unwrap_or(DEFAULT_TITLE));
        mapping
    }
}

pub fn is_main_role(role: &str) -> bool {
    role == MAIN_ROLE
}

pub fn role_of(mapping: &Mapping) -> Option<String> {
    frontmatter::tags_with_prefix(mapping, ROLE_NAMESPACE)
        .into_iter()
        .next()
        .and_then(|tag| tag.strip_prefix("note/").map(ToString::to_string))
}

pub fn parse_role(value: &str) -> Result<String, String> {
    let value = value.trim().trim_start_matches("note/");
    if value.is_empty() {
        return Err("note role must be non-empty".to_string());
    }
    if value.contains('/') || value.chars().any(char::is_whitespace) {
        return Err(format!("invalid note role: {value}"));
    }
    Ok(value.to_string())
}

/// Flat destination under `tasks/` for a note that has been moved already.
pub fn destination_path(notebook: &Path, file_name: &str) -> PathBuf {
    notebook.join(TASKS_DIR).join(file_name)
}

pub use frontmatter::{atomic_write, parse, render};

#[cfg(test)]
mod test;
