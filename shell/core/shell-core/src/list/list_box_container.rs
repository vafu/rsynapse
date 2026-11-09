//! ListBox backend for reconciled child components.
use std::ptr::NonNull;
use std::rc::Rc;

use gtk::glib::Quark;
use gtk::prelude::{ListBoxRowExt, ListModelExt, ObjectExt, WidgetExt};
use relm4::{Component, ComponentController, Controller, Sender};
use shell_source::{ListChange, ListDiff, diff_slices, diff_slices_by_key};

use super::{ComponentListBoxExt, ComponentListUpdate};

impl ComponentListBoxExt for gtk::ListBox {
    fn set_component_list<C>(&self, update: ComponentListUpdate<'_, C>)
    where
        C: Component,
        C::Init: Clone + PartialEq + 'static,
        C::Root: AsRef<gtk::Widget> + Clone + std::fmt::Debug,
    {
        let container = self;
        let key = component_list_key::<C>();
        let host = component_list_host::<C>(container, key);
        host.set_launcher(None);
        host.reconcile(container, update.items);
    }

    fn set_component_list_by_key<C, K, F>(&self, update: ComponentListUpdate<'_, C>, key: F)
    where
        C: Component,
        C::Init: Clone + PartialEq + 'static,
        K: Eq + std::hash::Hash + 'static,
        F: Fn(&C::Init) -> K + 'static,
        C::Root: AsRef<gtk::Widget> + Clone + std::fmt::Debug,
    {
        let container = self;
        let quark = component_list_key::<C>();
        let host = component_list_host::<C>(container, quark);
        host.set_launcher(None);
        host.reconcile_by_key(container, update.items, &key);
    }

    fn set_component_list_forward<C, O, F>(
        &self,
        update: ComponentListUpdate<'_, C>,
        output: &Sender<O>,
        forward: F,
    ) where
        C: Component,
        C::Init: Clone + PartialEq + 'static,
        C::Output: 'static,
        C::Root: AsRef<gtk::Widget> + Clone + std::fmt::Debug,
        O: 'static,
        F: Fn(C::Output) -> O + Clone + 'static,
    {
        let container = self;
        let key = component_list_key::<C>();
        let host = component_list_host::<C>(container, key);
        let output = output.clone();
        host.set_launcher(Some(Rc::new(move |item, id| {
            ComponentListRow::new_forward(item, id, output.clone(), forward.clone())
        })));
        host.reconcile(container, update.items);
    }

    fn set_component_list_by_key_forward<C, K, F, O, M>(
        &self,
        update: ComponentListUpdate<'_, C>,
        key: F,
        output: &Sender<O>,
        forward: M,
    ) where
        C: Component,
        C::Init: Clone + PartialEq + 'static,
        C::Output: 'static,
        C::Root: AsRef<gtk::Widget> + Clone + std::fmt::Debug,
        K: Eq + std::hash::Hash + 'static,
        F: Fn(&C::Init) -> K + 'static,
        O: 'static,
        M: Fn(C::Output) -> O + Clone + 'static,
    {
        let container = self;
        let quark = component_list_key::<C>();
        let host = component_list_host::<C>(container, quark);
        let output = output.clone();
        host.set_launcher(Some(Rc::new(move |item, id| {
            ComponentListRow::new_forward(item, id, output.clone(), forward.clone())
        })));
        host.reconcile_by_key(container, update.items, &key);
    }
}

struct ComponentListHost<C>
where
    C: Component,
{
    rows: Vec<ComponentListRow<C>>,
    next_row_id: u64,
    launcher: Option<Rc<RowLauncher<C>>>,
}

impl<C> Default for ComponentListHost<C>
where
    C: Component,
{
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            next_row_id: 1,
            launcher: None,
        }
    }
}

type RowLauncher<C> = dyn Fn(<C as Component>::Init, u64) -> ComponentListRow<C>;

impl<C> ComponentListHost<C>
where
    C: Component,
    C::Init: Clone + PartialEq + 'static,
    C::Root: AsRef<gtk::Widget> + Clone + std::fmt::Debug,
{
    fn set_launcher(&mut self, launcher: Option<Rc<RowLauncher<C>>>) {
        self.launcher = launcher;
    }

    fn reconcile(&mut self, container: &gtk::ListBox, items: &[C::Init])
    where
        C::Init: PartialEq,
    {
        if self.rows.len() == items.len()
            && self
                .rows
                .iter()
                .zip(items)
                .all(|(row, item)| &row.item == item)
        {
            return;
        }
        let previous = self
            .rows
            .iter()
            .map(|row| row.item.clone())
            .collect::<Vec<_>>();
        let diff = diff_slices(&previous, items);
        self.apply_diff(container, items, diff);
    }

    fn reconcile_by_key<K, F>(&mut self, container: &gtk::ListBox, items: &[C::Init], key: &F)
    where
        K: Eq + std::hash::Hash,
        F: Fn(&C::Init) -> K,
    {
        let previous = self
            .rows
            .iter()
            .map(|row| row.item.clone())
            .collect::<Vec<_>>();
        let diff = diff_slices_by_key(&previous, items, key);
        self.apply_diff(container, items, diff);
        for (row, item) in self.rows.iter_mut().zip(items) {
            row.item = item.clone();
        }
    }

    fn apply_diff(
        &mut self,
        container: &gtk::ListBox,
        _items: &[C::Init],
        diff: ListDiff<C::Init>,
    ) {
        let selected = container.selected_rows().into_iter().next();
        let selected_root = selected.as_ref().and_then(|selected| {
            let selected_widget: &gtk::Widget = selected.as_ref();
            self.rows
                .iter()
                .find(|row| {
                    row.widget() == selected_widget
                        || list_box_wrapper(container, row.widget()).as_ref() == Some(selected)
                })
                .map(|row| row.widget().clone())
        });
        let mut rows = std::mem::take(&mut self.rows);
        for change in diff.changes {
            match change {
                ListChange::Insert { index, item } => {
                    let id = self.next_row_id;
                    self.next_row_id += 1;
                    let row = match &self.launcher {
                        Some(launcher) => launcher(item, id),
                        None => ComponentListRow::new(item, id),
                    };
                    rows.insert(index.min(rows.len()), row);
                }
                ListChange::Remove { index } => {
                    if let Some(row) = rows.get(index)
                        && let Some(wrapper) = list_box_wrapper(container, row.widget())
                    {
                        container.remove(&wrapper);
                    }
                    if index < rows.len() {
                        rows.remove(index);
                    }
                }
                ListChange::Move { from, to } => {
                    if from < rows.len() {
                        let row = rows.remove(from);
                        rows.insert(to.min(rows.len()), row);
                    }
                }
            }
        }
        reconcile_list_box_order(container, &rows);
        if let Some(root) = selected_root
            && let Some(row) = list_box_row_for_root(container, &root)
        {
            container.select_row(Some(&row));
        }
        self.rows = rows;
    }
}

fn reconcile_list_box_order<C>(container: &gtk::ListBox, rows: &[ComponentListRow<C>])
where
    C: Component,
    C::Init: Clone,
    C::Root: AsRef<gtk::Widget>,
{
    // Remove wrappers explicitly so each root is unparented before re-append.
    // GtkListBox then rebuilds wrappers in the reconciled order; selection is
    // restored by root identity after this.
    while let Some(wrapper) = container.row_at_index(0) {
        container.remove(&wrapper);
    }
    for row in rows {
        if row.widget().parent().is_some() {
            row.widget().unparent();
        }
        container.append(row.widget());
    }
}

fn list_box_wrapper(container: &gtk::ListBox, widget: &gtk::Widget) -> Option<gtk::ListBoxRow> {
    (0..container.observe_children().n_items() as i32)
        .filter_map(|index| container.row_at_index(index))
        .find(|row| row.child().as_ref() == Some(widget))
}

fn list_box_row_for_root(container: &gtk::ListBox, root: &gtk::Widget) -> Option<gtk::ListBoxRow> {
    (0..container.observe_children().n_items() as i32)
        .filter_map(|index| container.row_at_index(index))
        .find(|row| {
            let row_widget: &gtk::Widget = row.as_ref();
            row_widget == root || row.child().as_ref() == Some(root)
        })
}

struct ComponentListRow<C>
where
    C: Component,
{
    item: C::Init,
    controller: Controller<C>,
}

impl<C> ComponentListRow<C>
where
    C: Component,
    C::Init: Clone,
{
    fn new(item: C::Init, id: u64) -> Self {
        let _ = id;
        let controller = C::builder().launch(item.clone()).detach();
        Self { item, controller }
    }

    fn new_forward<O, F>(item: C::Init, id: u64, output: Sender<O>, forward: F) -> Self
    where
        C::Output: 'static,
        O: 'static,
        F: Fn(C::Output) -> O + Clone + 'static,
    {
        let _ = id;
        let controller = C::builder().launch(item.clone()).forward(&output, forward);
        Self { item, controller }
    }

    fn widget(&self) -> &gtk::Widget
    where
        C::Root: AsRef<gtk::Widget>,
    {
        self.controller.widget().as_ref()
    }
}

fn component_list_key<C>() -> Quark
where
    C: Component,
{
    Quark::from_str(std::any::type_name::<ComponentListHost<C>>())
}

#[allow(clippy::mut_from_ref)]
fn component_list_host<C>(container: &gtk::ListBox, key: Quark) -> &mut ComponentListHost<C>
where
    C: Component,
{
    unsafe {
        if container.qdata::<ComponentListHost<C>>(key).is_none() {
            container.set_qdata(key, ComponentListHost::<C>::default());
        }
        let host: NonNull<ComponentListHost<C>> = container.qdata(key).expect("host installed");
        host.as_ptr().as_mut().expect("host pointer valid")
    }
}
