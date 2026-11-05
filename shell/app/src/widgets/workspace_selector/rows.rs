//! Row model and rendering for the workspace selector.
use adw::prelude::*;
use shell_core::gtk;

use super::source::View;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum EntryKey {
    Project(String),
    Unassign,
    Rename,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Entry {
    Project {
        id: String,
        name: String,
        path: String,
        current: bool,
    },
    Unassign,
    Rename {
        name: String,
    },
}

impl Entry {
    pub(super) fn key(&self) -> EntryKey {
        match self {
            Self::Project { id, .. } => EntryKey::Project(id.clone()),
            Self::Unassign => EntryKey::Unassign,
            Self::Rename { .. } => EntryKey::Rename,
        }
    }
}

pub(super) fn entries(view: &View, query: &str) -> Vec<Entry> {
    let query = query.trim();
    let words = query.to_lowercase();
    let words = words.split_whitespace().collect::<Vec<_>>();

    let mut entries = view
        .projects
        .iter()
        .filter(|project| {
            let text = format!("{} {}", project.name, project.path).to_lowercase();
            words.iter().all(|word| text.contains(word))
        })
        .map(|project| Entry::Project {
            id: project.id.clone(),
            name: project.name.clone(),
            path: project.path.clone(),
            current: view.current_project_id.as_deref() == Some(project.id.as_str()),
        })
        .collect::<Vec<_>>();

    if query.is_empty() && view.current_project_id.is_some() {
        entries.push(Entry::Unassign);
    }
    if !query.is_empty() {
        entries.push(Entry::Rename {
            name: query.to_owned(),
        });
    }

    entries
}

pub(super) fn row(entry: &Entry) -> adw::ActionRow {
    let (icon, title, subtitle) = match entry {
        Entry::Project {
            name,
            path,
            current,
            ..
        } => (
            "folder-symbolic",
            name.clone(),
            if *current {
                format!("Current project · {path}")
            } else {
                path.clone()
            },
        ),
        Entry::Unassign => (
            "list-remove-symbolic",
            "Unassign project".to_owned(),
            "Remove the project association from this workspace".to_owned(),
        ),
        Entry::Rename { name } => (
            "document-edit-symbolic",
            format!("Rename to “{name}”"),
            "Set the workspace name".to_owned(),
        ),
    };

    let row = adw::ActionRow::builder()
        .title(&title)
        .subtitle(&subtitle)
        .title_lines(1)
        .subtitle_lines(1)
        .activatable(true)
        .build();
    row.set_use_markup(false);
    row.add_css_class("workspace-selector-row");

    let icon = gtk::Image::from_icon_name(icon);
    icon.set_valign(gtk::Align::Center);
    row.add_prefix(&icon);

    if let Entry::Project { current: true, .. } = entry {
        let check = gtk::Image::from_icon_name("object-select-symbolic");
        check.add_css_class("accent");
        check.set_valign(gtk::Align::Center);
        row.add_suffix(&check);
    }

    row
}

#[cfg(test)]
mod tests {
    use super::{Entry, entries};
    use crate::widgets::workspace_selector::source::{Project, View};

    fn view() -> View {
        View {
            workspace_id: Some("w".into()),
            workspace_name: Some("noble-owl".into()),
            current_project_id: Some("p1".into()),
            current_project_name: Some("Rsynapse".into()),
            projects: vec![
                Project {
                    id: "p1".into(),
                    name: "Rsynapse".into(),
                    path: "/repo/rsynapse".into(),
                },
                Project {
                    id: "p2".into(),
                    name: "Mobile".into(),
                    path: "/repo/mobile".into(),
                },
            ],
        }
    }

    #[test]
    fn filters_projects_and_keeps_rename_last() {
        let entries = entries(&view(), "mob");
        assert_eq!(
            entries,
            vec![
                Entry::Project {
                    id: "p2".into(),
                    name: "Mobile".into(),
                    path: "/repo/mobile".into(),
                    current: false,
                },
                Entry::Rename { name: "mob".into() },
            ]
        );
    }

    #[test]
    fn empty_query_shows_current_project_and_unassign() {
        let entries = entries(&view(), "  ");
        assert_eq!(entries.len(), 3);
        assert!(matches!(&entries[0], Entry::Project { current: true, .. }));
        assert_eq!(entries[2], Entry::Unassign);
    }

    #[test]
    fn rename_preserves_typed_case_and_whitespace_is_trimmed() {
        let entries = entries(&view(), "  New Name  ");
        assert_eq!(
            entries.last(),
            Some(&Entry::Rename {
                name: "New Name".into()
            })
        );
    }
}
