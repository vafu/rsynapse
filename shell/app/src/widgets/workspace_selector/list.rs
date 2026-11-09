//! Keyed list reconciliation and selection for the workspace selector.
use relm4::prelude::*;
use shell_core::{
    gtk::prelude::*,
    list::{ComponentListBoxExt, ComponentListUpdate},
};

use super::{
    WorkspaceSelector,
    rows::{self, EntryKey, WorkspaceEntryRow},
};

impl WorkspaceSelector {
    pub(super) fn refresh_entries(&mut self) {
        let entries = rows::sync_entries(&mut self.row_states, &self.view, &self.query);
        self.entries = entries;
        self.reconcile_list();
        self.sync_selection();
    }

    fn reconcile_list(&self) {
        let Some(list) = self.list.as_ref() else {
            return;
        };
        list.set_component_list_by_key(
            ComponentListUpdate::<WorkspaceEntryRow>::new(&self.entries),
            |entry| entry.key(),
        );
    }

    fn sync_selection(&self) {
        let selected = self
            .selected_key
            .get()
            .filter(|key| self.entries.iter().any(|entry| entry.key() == *key))
            .or_else(|| self.default_selection_key());
        self.selected_key.set(selected.clone());

        let Some(list) = self.list.as_ref() else {
            return;
        };
        self.suppress_selection.set(true);
        match selected.and_then(|key| self.row_for_key(&key)) {
            Some(row) => {
                list.select_row(Some(&row));
                if let Some(scrolled) = self.scrolled.as_ref() {
                    shell_core::list::scroll_row_into_view(scrolled, &row);
                }
            }
            None => list.unselect_all(),
        }
        self.suppress_selection.set(false);
    }

    fn default_selection_key(&self) -> Option<EntryKey> {
        if self.entries.is_empty() {
            return None;
        }
        if self.query.trim().is_empty()
            && let Some(current) = self
                .entries
                .iter()
                .find(|entry| entry.state().get().is_some_and(|state| state.current()))
        {
            return Some(current.key());
        }
        self.entries.first().map(|entry| entry.key())
    }

    pub(super) fn move_selection(&self, delta: i32) {
        if self.entries.is_empty() {
            return;
        }

        let count = self.entries.len() as i32;
        let current = self
            .selected_key
            .get()
            .and_then(|key| rows::entry_index(&self.entries, &key))
            .map(|index| index as i32);
        let next = match current {
            Some(index) if index >= 0 => (index + delta).rem_euclid(count),
            _ if delta < 0 => count - 1,
            _ => 0,
        };
        self.selected_key
            .set(self.entries.get(next as usize).map(|entry| entry.key()));
        self.sync_selection();
    }

    pub(super) fn selected_entry(&self) -> Option<rows::EntryState> {
        self.selected_key.get().and_then(|key| {
            self.entries
                .iter()
                .find(|entry| entry.key() == key)
                .and_then(|entry| entry.state().get())
        })
    }

    pub(super) fn selected_project_id(&self) -> Option<String> {
        match self.selected_key.get()? {
            EntryKey::Project(id) => Some(id),
            _ => None,
        }
    }

    fn row_for_key(&self, key: &EntryKey) -> Option<gtk::ListBoxRow> {
        let list = self.list.as_ref()?;
        let target = key.row_name();
        (0..list.observe_children().n_items() as i32)
            .filter_map(|index| list.row_at_index(index))
            .find(|row| rows::row_identity(row) == target)
    }
}
