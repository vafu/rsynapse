//! Structural row identity and the reactive row component for the workspace selector.
use std::collections::HashMap;

use adw::prelude::*;
use relm4::prelude::*;
use shell_core::{gtk, source::StateSignal};

use super::source::View;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(super) enum EntryKey {
    Project(String),
    Unassign,
    Rename,
}

impl EntryKey {
    pub(super) fn row_name(&self) -> String {
        match self {
            Self::Project(id) => format!("project/{id}"),
            Self::Unassign => "unassign".to_owned(),
            Self::Rename => "rename".to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum EntryState {
    Project {
        name: String,
        path: String,
        current: bool,
    },
    Unassign,
    Rename {
        name: String,
    },
}

impl EntryState {
    pub(super) fn icon(&self) -> &'static str {
        match self {
            Self::Project { .. } => "folder-symbolic",
            Self::Unassign => "list-remove-symbolic",
            Self::Rename { .. } => "document-edit-symbolic",
        }
    }

    pub(super) fn title(&self) -> String {
        match self {
            Self::Project { name, .. } => name.clone(),
            Self::Unassign => "Unassign project".to_owned(),
            Self::Rename { name } => format!("Rename to “{name}”"),
        }
    }

    pub(super) fn subtitle(&self) -> String {
        match self {
            Self::Project {
                path,
                current: true,
                ..
            } => format!("Current project · {path}"),
            Self::Project { path, .. } => path.clone(),
            Self::Unassign => "Remove the project association from this workspace".to_owned(),
            Self::Rename { .. } => "Set the workspace name".to_owned(),
        }
    }

    pub(super) fn current(&self) -> bool {
        matches!(self, Self::Project { current: true, .. })
    }
}

#[derive(Clone, Debug)]
pub(super) struct EntryInit {
    key: EntryKey,
    state: StateSignal<Option<EntryState>>,
}

impl EntryInit {
    pub(super) fn key(&self) -> EntryKey {
        self.key.clone()
    }

    pub(super) fn state(&self) -> StateSignal<Option<EntryState>> {
        self.state.clone()
    }
}

impl PartialEq for EntryInit {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.state == other.state
    }
}

impl Eq for EntryInit {}

pub(super) fn sync_entries(
    states: &mut HashMap<EntryKey, StateSignal<Option<EntryState>>>,
    view: &View,
    query: &str,
) -> Vec<EntryInit> {
    let keys = entry_keys(view, query);
    states.retain(|key, _| keys.iter().any(|candidate| candidate == key));

    keys.into_iter()
        .map(|key| {
            let state = states
                .entry(key.clone())
                .or_insert_with(|| StateSignal::new(None));
            state.set(Some(entry_state(view, query, &key)));
            EntryInit {
                key,
                state: state.clone(),
            }
        })
        .collect()
}

pub(super) fn entry_keys(view: &View, query: &str) -> Vec<EntryKey> {
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
        .map(|project| EntryKey::Project(project.id.clone()))
        .collect::<Vec<_>>();

    if query.is_empty() && view.current_project_id.is_some() {
        entries.push(EntryKey::Unassign);
    }
    if !query.is_empty() {
        entries.push(EntryKey::Rename);
    }

    entries
}

fn entry_state(view: &View, query: &str, key: &EntryKey) -> EntryState {
    match key {
        EntryKey::Project(id) => view
            .projects
            .iter()
            .find(|project| project.id == *id)
            .map(|project| EntryState::Project {
                name: project.name.clone(),
                path: project.path.clone(),
                current: view.current_project_id.as_deref() == Some(project.id.as_str()),
            })
            .unwrap_or_else(|| EntryState::Project {
                name: id.clone(),
                path: String::new(),
                current: false,
            }),
        EntryKey::Unassign => EntryState::Unassign,
        EntryKey::Rename => EntryState::Rename {
            name: query.trim().to_owned(),
        },
    }
}

pub(super) fn entry_index(entries: &[EntryInit], key: &EntryKey) -> Option<usize> {
    entries.iter().position(|entry| entry.key == *key)
}

pub(super) fn row_identity(row: &gtk::ListBoxRow) -> String {
    if !row.widget_name().is_empty() {
        return row.widget_name().to_string();
    }
    row.child()
        .map(|child| child.widget_name().to_string())
        .unwrap_or_default()
}

fn state_icon(state: Option<&EntryState>) -> &'static str {
    state.map_or("folder-symbolic", EntryState::icon)
}

fn state_title(state: Option<&EntryState>) -> String {
    state.map_or_else(String::new, EntryState::title)
}

fn state_subtitle(state: Option<&EntryState>) -> String {
    state.map_or_else(String::new, EntryState::subtitle)
}

fn state_current(state: Option<&EntryState>) -> bool {
    state.is_some_and(EntryState::current)
}

#[derive(Debug)]
#[shell_macros::model(module = workspace_entry_row_sources)]
pub(super) struct WorkspaceEntryRow {
    key: EntryKey,
    row_name: String,
    icon: gtk::Image,
    check: gtk::Image,
    state_signal: StateSignal<Option<EntryState>>,

    #[source(state_signal.observable())]
    state: Option<EntryState>,
}

#[shell_macros::component(module = workspace_entry_row_sources, model = WorkspaceEntryRow)]
#[relm4::component(pub(crate))]
impl SimpleComponent for WorkspaceEntryRow {
    type Init = EntryInit;
    type Input = workspace_entry_row_sources::Msg;
    type Output = ();

    view! {
        #[root]
        adw::ActionRow {
            set_activatable: true,
            set_selectable: true,
            set_title_lines: 1,
            set_subtitle_lines: 1,
            set_widget_name: model.row_name.as_str(),
            add_css_class: "workspace-selector-row",
            #[watch]
            set_title: state_title(model.state.as_ref()).as_str(),
            #[watch]
            set_subtitle: state_subtitle(model.state.as_ref()).as_str(),
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let state = init.state.get();
        let icon = gtk::Image::from_icon_name(state_icon(state.as_ref()));
        icon.set_valign(gtk::Align::Center);
        let check = gtk::Image::from_icon_name("object-select-symbolic");
        check.add_css_class("accent");
        check.set_valign(gtk::Align::Center);
        check.set_visible(state_current(state.as_ref()));

        let mut model = WorkspaceEntryRow::new(
            init.key.clone(),
            init.key.row_name(),
            icon,
            check,
            init.state,
        );
        model
            .icon
            .set_icon_name(Some(state_icon(model.state.as_ref())));
        model.check.set_visible(state_current(model.state.as_ref()));

        let widgets = view_output!();
        root.add_prefix(&model.icon);
        root.add_suffix(&model.check);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        WorkspaceEntryRow::update(self, message);
        self.icon
            .set_icon_name(Some(state_icon(self.state.as_ref())));
        self.check.set_visible(state_current(self.state.as_ref()));
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use shell_core::{
        gtk::{self, prelude::*},
        list::{ComponentListBoxExt, ComponentListUpdate},
    };

    use super::{
        EntryKey, EntryState, WorkspaceEntryRow, entry_index, entry_keys, row_identity,
        sync_entries,
    };
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
        let entries = entry_keys(&view(), "mob");
        assert_eq!(
            entries,
            vec![EntryKey::Project("p2".into()), EntryKey::Rename]
        );
    }

    #[test]
    fn empty_query_shows_current_project_and_unassign() {
        let entries = entry_keys(&view(), "  ");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0], EntryKey::Project("p1".into()));
        assert_eq!(entries[2], EntryKey::Unassign);
    }

    #[test]
    fn rename_preserves_typed_case_and_whitespace_is_trimmed() {
        let mut states = HashMap::new();
        let entries = sync_entries(&mut states, &view(), "  New Name  ");
        let rename = entries.last().expect("rename row");
        assert_eq!(
            rename.state().get(),
            Some(EntryState::Rename {
                name: "New Name".into()
            })
        );
    }

    #[test]
    fn same_key_update_reuses_row_signal() {
        let mut states = HashMap::new();
        let first = sync_entries(&mut states, &view(), "");
        let first_signal = first[0].state();

        let mut changed = view();
        changed.projects[0].name = "Renamed".into();
        changed.projects[0].path = "/repo/renamed".into();
        let second = sync_entries(&mut states, &changed, "");

        assert_eq!(first[0].key(), second[0].key());
        assert_eq!(first_signal, second[0].state());
        assert_eq!(
            second[0].state().get(),
            Some(EntryState::Project {
                name: "Renamed".into(),
                path: "/repo/renamed".into(),
                current: true,
            })
        );
    }

    #[gtk::test]
    fn keyed_reconciliation_reuses_row_widget_on_live_state_update() {
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.set_visible(false);

        let mut states = HashMap::new();
        let first = sync_entries(&mut states, &view(), "");
        list.set_component_list_by_key(
            ComponentListUpdate::<WorkspaceEntryRow>::new(&first),
            |entry| entry.key(),
        );
        let before = list.row_at_index(0).expect("row widget");

        let mut changed = view();
        changed.projects[0].name = "Renamed".into();
        let second = sync_entries(&mut states, &changed, "");
        list.set_component_list_by_key(
            ComponentListUpdate::<WorkspaceEntryRow>::new(&second),
            |entry| entry.key(),
        );
        let after = list.row_at_index(0).expect("row widget after update");

        assert_eq!(before, after);
        assert_eq!(first[0].state(), second[0].state());
    }

    #[gtk::test]
    fn list_selection_survives_reorder_by_key() {
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.set_visible(false);

        let mut states = HashMap::new();
        let first = sync_entries(&mut states, &view(), "");
        list.set_component_list_by_key(
            ComponentListUpdate::<WorkspaceEntryRow>::new(&first),
            |entry| entry.key(),
        );
        let selected = list.row_at_index(1).expect("p2 row");
        list.select_row(Some(&selected));
        assert_eq!(row_identity(&selected), "project/p2");

        let mut reordered = view();
        reordered.projects.reverse();
        let second = sync_entries(&mut states, &reordered, "");
        list.set_component_list_by_key(
            ComponentListUpdate::<WorkspaceEntryRow>::new(&second),
            |entry| entry.key(),
        );

        let selected = list
            .selected_rows()
            .into_iter()
            .next()
            .expect("selection preserved");
        assert_eq!(row_identity(&selected), "project/p2");
    }

    #[test]
    fn selection_follows_key_after_reorder() {
        let mut states = HashMap::new();
        let first = sync_entries(&mut states, &view(), "");
        let selected = EntryKey::Project("p2".into());
        assert_eq!(entry_index(&first, &selected), Some(1));

        let mut reordered = view();
        reordered.projects.reverse();
        let second = sync_entries(&mut states, &reordered, "");
        assert_eq!(entry_index(&second, &selected), Some(0));
        assert_eq!(second[0].state(), first[1].state());
    }
}
