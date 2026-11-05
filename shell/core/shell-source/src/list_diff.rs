//! Observable collection diffing for incremental list updates.
use futures_util::{StreamExt, stream};
use rxrust::prelude::Observable as _;

use crate::Observable;

/// One sequential edit applied to a list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListChange<T> {
    Insert { index: usize, item: T },
    Remove { index: usize },
    Move { from: usize, to: usize },
}

/// Ordered edits that transform the previous list into the current one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListDiff<T> {
    pub changes: Vec<ListChange<T>>,
}

impl<T> Default for ListDiff<T> {
    fn default() -> Self {
        Self {
            changes: Vec::new(),
        }
    }
}

impl<T> ListDiff<T> {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Applies this diff to a plain list. Indices refer to the list state
    /// immediately before each individual edit.
    pub fn apply(&self, items: &mut Vec<T>)
    where
        T: Clone,
    {
        for change in &self.changes {
            match change {
                ListChange::Insert { index, item } => {
                    items.insert((*index).min(items.len()), item.clone());
                }
                ListChange::Remove { index } => {
                    if *index < items.len() {
                        items.remove(*index);
                    }
                }
                ListChange::Move { from, to } => {
                    if *from < items.len() {
                        let item = items.remove(*from);
                        items.insert((*to).min(items.len()), item);
                    }
                }
            }
        }
    }
}

/// Emits the sequential edit list between consecutive source values.
///
/// The first emission is a diff from the empty list. Equal items are retained
/// by identity (including across reorders), so consumers can update rows in
/// place instead of rebuilding the whole collection.
pub fn diff<T>(source: Observable<Vec<T>>) -> Observable<ListDiff<T>>
where
    T: Clone + PartialEq + Send + 'static,
{
    let stream = stream::unfold(
        (source.into_stream(), Vec::<T>::new()),
        |(mut source, previous)| async move {
            while let Some(item) = source.next().await {
                match item {
                    Ok(next) => {
                        let diff = diff_slices(&previous, &next);
                        return Some((Ok(diff), (source, next)));
                    }
                    Err(error) => return Some((Err(error), (source, previous))),
                }
            }
            None
        },
    );
    crate::support::from_stream_result(stream)
}

/// Computes a move-aware diff between two slices.
pub fn diff_slices<T>(previous: &[T], next: &[T]) -> ListDiff<T>
where
    T: Clone + PartialEq,
{
    let old_len = previous.len();
    let new_len = next.len();
    let mut used_old = vec![false; old_len];
    let mut new_to_old = vec![None; new_len];

    // Prefer stable items first, then match remaining equal items by proximity.
    for index in 0..old_len.min(new_len) {
        if previous[index] == next[index] {
            used_old[index] = true;
            new_to_old[index] = Some(index);
        }
    }
    for new_index in 0..new_len {
        if new_to_old[new_index].is_some() {
            continue;
        }
        let mut best: Option<usize> = None;
        for old_index in 0..old_len {
            if used_old[old_index] || previous[old_index] != next[new_index] {
                continue;
            }
            let distance = old_index.abs_diff(new_index);
            if best.is_none_or(|best_index| distance < best_index.abs_diff(new_index)) {
                best = Some(old_index);
            }
        }
        if let Some(old_index) = best {
            used_old[old_index] = true;
            new_to_old[new_index] = Some(old_index);
        }
    }

    // `current` tracks old-item identities and `None` for new insertions.
    let mut current = (0..old_len).map(Some).collect::<Vec<_>>();
    let mut changes = Vec::new();

    for old_index in (0..old_len).rev() {
        if used_old[old_index] {
            continue;
        }
        if let Some(position) = current.iter().position(|item| *item == Some(old_index)) {
            changes.push(ListChange::Remove { index: position });
            current.remove(position);
        }
    }

    for (new_index, item) in next.iter().enumerate() {
        match new_to_old[new_index] {
            Some(old_index) => {
                let position = current
                    .iter()
                    .position(|item| *item == Some(old_index))
                    .expect("matched item must remain in the working list");
                if position != new_index {
                    changes.push(ListChange::Move {
                        from: position,
                        to: new_index,
                    });
                    let item = current.remove(position);
                    current.insert(new_index, item);
                }
            }
            None => {
                changes.push(ListChange::Insert {
                    index: new_index,
                    item: item.clone(),
                });
                current.insert(new_index, None);
            }
        }
    }

    ListDiff { changes }
}

#[cfg(test)]
mod tests {
    use super::{ListChange, diff_slices};

    fn apply<T: Clone + PartialEq>(previous: &[T], next: &[T]) -> Vec<T> {
        let mut items = previous.to_vec();
        diff_slices(previous, next).apply(&mut items);
        items
    }

    #[test]
    fn inserts_from_empty() {
        assert_eq!(
            diff_slices(&[], &["a", "b"]).changes,
            vec![
                ListChange::Insert {
                    index: 0,
                    item: "a"
                },
                ListChange::Insert {
                    index: 1,
                    item: "b"
                },
            ]
        );
    }

    #[test]
    fn removes_only_the_deleted_item() {
        assert_eq!(
            diff_slices(&["a", "b", "c"], &["a", "c"]).changes,
            vec![ListChange::Remove { index: 1 }]
        );
    }

    #[test]
    fn inserts_in_the_middle() {
        assert_eq!(
            diff_slices(&["a", "c"], &["a", "b", "c"]).changes,
            vec![ListChange::Insert {
                index: 1,
                item: "b"
            }]
        );
    }

    #[test]
    fn moved_items_are_reported_without_recreation() {
        assert_eq!(
            diff_slices(&["a", "b", "c"], &["c", "a", "b"]).changes,
            vec![ListChange::Move { from: 2, to: 0 }]
        );
    }

    #[test]
    fn diff_slices_is_stateless_across_threads() {
        use std::{sync::Arc, thread};

        let cases = Arc::new(
            (0..256)
                .map(|seed| {
                    let previous = (0..(seed % 7))
                        .map(|index| (index * seed + 3) % 5)
                        .collect::<Vec<_>>();
                    let target = (0..((seed * 3) % 8))
                        .map(|index| (index * seed + 1) % 5)
                        .collect::<Vec<_>>();
                    (previous, target)
                })
                .collect::<Vec<_>>(),
        );

        let handles = (0..8)
            .map(|_| {
                let cases = cases.clone();
                thread::spawn(move || {
                    for (previous, target) in cases.iter() {
                        let mut items = previous.clone();
                        diff_slices(previous, target).apply(&mut items);
                        assert_eq!(&items, target);
                    }
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn exhaustive_small_sequences_apply_to_the_target() {
        fn sequences(max_len: usize) -> Vec<Vec<&'static str>> {
            let mut all = vec![Vec::new()];
            for _ in 0..max_len {
                let previous = all.clone();
                for prefix in previous {
                    for item in ["a", "b", "c"] {
                        let mut next = prefix.clone();
                        next.push(item);
                        all.push(next);
                    }
                }
            }
            all
        }

        for previous in sequences(5) {
            for target in sequences(5) {
                let diff = diff_slices(&previous, &target);
                let mut items = previous.clone();
                diff.apply(&mut items);
                assert_eq!(
                    items, target,
                    "previous={previous:?} target={target:?} diff={diff:?}"
                );
            }
        }
    }

    #[test]
    fn unchanged_lists_produce_an_empty_diff() {
        assert!(diff_slices(&["a", "b"], &["a", "b"]).is_empty());
    }

    #[test]
    fn applying_changes_reconstructs_the_target_list() {
        let previous = ["a", "b", "c", "d"];
        let target = ["d", "b", "x", "a"];
        assert_eq!(apply(&previous, &target), target);
    }
}
