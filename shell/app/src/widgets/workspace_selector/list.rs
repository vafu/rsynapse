//! Incremental GTK list updates for the workspace selector.
use relm4::prelude::*;
use shell_core::{gtk::prelude::*, source::ListChange};

use super::{
    WorkspaceSelector,
    rows::{self, Entry},
};

impl WorkspaceSelector {
    pub(super) fn apply_list_diff(&mut self) {
        let diff = self.list_diff.clone();
        if diff.is_empty() {
            self.empty.set_visible(self.entries.is_empty());
            return;
        }
        let selected_index = self
            .list
            .selected_row()
            .map(|row| row.index())
            .filter(|index| *index >= 0)
            .map(|index| index as usize);
        let selected_key = selected_index
            .and_then(|index| self.entries.get(index))
            .map(Entry::key);

        let mut entries = self.entries.clone();
        for change in &diff.changes {
            match change {
                ListChange::Insert { index, item } => {
                    let index = (*index).min(self.list.observe_children().n_items() as usize);
                    self.list.insert(&rows::row(item), index as i32);
                }
                ListChange::Remove { index } => {
                    if let Some(row) = self.list.row_at_index(*index as i32) {
                        self.list.remove(&row);
                    }
                }
                ListChange::Move { from, to } => {
                    if let Some(row) = self.list.row_at_index(*from as i32) {
                        self.list.remove(&row);
                        let position =
                            (*to).min(self.list.observe_children().n_items() as usize) as i32;
                        self.list.insert(&row, position);
                    }
                }
            }
        }
        diff.apply(&mut entries);
        self.entries = entries;
        self.empty.set_visible(self.entries.is_empty());

        let target = selected_key
            .and_then(|key| self.entries.iter().position(|entry| entry.key() == key))
            .or_else(|| selected_index.map(|index| index.min(self.entries.len().saturating_sub(1))))
            .or_else(|| self.default_selection_index());

        if let Some(index) = target {
            self.select_index(index);
        } else {
            self.list.unselect_all();
        }
    }

    fn default_selection_index(&self) -> Option<usize> {
        if self.entries.is_empty() {
            return None;
        }
        if self.query.trim().is_empty() {
            if let Some(index) = self
                .entries
                .iter()
                .position(|entry| matches!(entry, Entry::Project { current: true, .. }))
            {
                return Some(index);
            }
        }
        Some(0)
    }

    pub(super) fn select_default(&self) {
        if let Some(index) = self.default_selection_index() {
            self.select_index(index);
        } else {
            self.list.unselect_all();
        }
    }

    pub(super) fn move_selection(&self, delta: i32) {
        if self.entries.is_empty() {
            return;
        }

        let count = self.entries.len() as i32;
        let current = self.list.selected_row().map(|row| row.index());
        let next = match current {
            Some(index) if index >= 0 => (index + delta).rem_euclid(count),
            _ if delta < 0 => count - 1,
            _ => 0,
        };
        self.select_index(next as usize);
    }

    fn select_index(&self, index: usize) {
        let Some(row) = self.list.row_at_index(index as i32) else {
            return;
        };
        self.list.select_row(Some(&row));
        self.scroll_to_row(&row);
    }

    fn scroll_to_row(&self, row: &gtk::ListBoxRow) {
        let Some(bounds) = row.compute_bounds(&self.list) else {
            return;
        };
        let adjustment = self.scrolled.vadjustment();
        let top = bounds.y() as f64;
        let bottom = top + bounds.height() as f64;
        if top < adjustment.value() {
            adjustment.set_value(top);
        } else if bottom > adjustment.value() + adjustment.page_size() {
            adjustment.set_value(bottom - adjustment.page_size());
        }
    }
}
