use std::{ptr::NonNull, sync::OnceLock};

use gtk::glib::Quark;
use gtk::prelude::{BoxExt, Cast, IsA, ObjectExt, WidgetExt};
use relm4::component::ComponentController;
use relm4::{Component, Controller};
use shell_source::{ListChange, ListDiff, diff_slices, diff_slices_by_key};

use super::{ComponentListBoxExt, ComponentListUpdate};

impl<T> ComponentListBoxExt for T
where
    T: IsA<gtk::Box>,
{
    fn set_component_list<C>(&self, update: ComponentListUpdate<'_, C>)
    where
        C: Component,
        C::Init: Clone + PartialEq + 'static,
        C::Root: AsRef<gtk::Widget> + Clone + std::fmt::Debug,
    {
        debug_assert!(
            gtk::glib::MainContext::default().is_owner(),
            "component list updates must run on the GTK main context"
        );
        let container = self.upcast_ref::<gtk::Box>();
        let key = component_list_key::<C>();
        let host = component_list_host::<C>(container, key);
        host.reconcile(container, update.items);
    }

    fn set_component_list_by_key<C, K, F>(&self, update: ComponentListUpdate<'_, C>, key: F)
    where
        C: Component,
        C::Init: Clone + 'static,
        K: Eq + std::hash::Hash + 'static,
        F: Fn(&C::Init) -> K + 'static,
        C::Root: AsRef<gtk::Widget> + Clone + std::fmt::Debug,
    {
        debug_assert!(
            gtk::glib::MainContext::default().is_owner(),
            "component list updates must run on the GTK main context"
        );
        let container = self.upcast_ref::<gtk::Box>();
        let quark = component_list_key::<C>();
        let host = component_list_host::<C>(container, quark);
        host.reconcile_by_key(container, update.items, &key);
    }
}

struct ComponentListHost<C>
where
    C: Component,
{
    rows: Vec<ComponentListRow<C>>,
    next_row_id: u64,
}

impl<C> Default for ComponentListHost<C>
where
    C: Component,
{
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            next_row_id: 1,
        }
    }
}

impl<C> ComponentListHost<C>
where
    C: Component,
    C::Init: Clone + 'static,
    C::Root: AsRef<gtk::Widget> + Clone + std::fmt::Debug,
{
    fn reconcile(&mut self, container: &gtk::Box, items: &[C::Init])
    where
        C::Init: PartialEq,
    {
        let row_type = std::any::type_name::<C>();
        let _span = tracing::trace_span!("list.reconcile", row = row_type).entered();
        let previous_len = self.rows.len();

        if previous_len == items.len()
            && self
                .rows
                .iter()
                .zip(items)
                .all(|(row, item)| &row.item == item)
        {
            tracing::trace!(
                previous = previous_len,
                next = items.len(),
                reused = items.len(),
                created = 0usize,
                removed = 0usize,
                "list unchanged"
            );
            trace_list_lifecycle::<C>(container, previous_len, items.len(), items.len(), 0, 0);
            return;
        }

        let previous_items = self
            .rows
            .iter()
            .map(|row| row.item.clone())
            .collect::<Vec<_>>();
        let diff = diff_slices(&previous_items, items);
        self.apply_diff(container, items, diff);
    }

    fn reconcile_by_key<K, F>(&mut self, container: &gtk::Box, items: &[C::Init], key: &F)
    where
        K: Eq + std::hash::Hash,
        F: Fn(&C::Init) -> K,
    {
        let previous_items = self
            .rows
            .iter()
            .map(|row| row.item.clone())
            .collect::<Vec<_>>();
        let diff = diff_slices_by_key(&previous_items, items, key);
        self.apply_diff(container, items, diff);
    }

    fn apply_diff(&mut self, container: &gtk::Box, items: &[C::Init], diff: ListDiff<C::Init>) {
        let previous_len = self.rows.len();
        let mut rows = std::mem::take(&mut self.rows);
        let mut created = 0usize;
        let mut removed = 0usize;
        let mut moved = 0usize;

        for change in diff.changes {
            match change {
                ListChange::Insert { index, item } => {
                    created += 1;
                    let id = self.next_row_id;
                    self.next_row_id += 1;
                    let index = index.min(rows.len());
                    trace_list_event::<C>(
                        container,
                        format_args!("create id={} index={}", id, index),
                    );
                    rows.insert(index, ComponentListRow::new(item, id));
                }
                ListChange::Remove { index } => {
                    if index < rows.len() {
                        removed += 1;
                        let row = rows.remove(index);
                        trace_list_event::<C>(
                            container,
                            format_args!("remove id={} index={}", row.id, index),
                        );
                        container.remove(row.widget());
                    }
                }
                ListChange::Move { from, to } => {
                    if from < rows.len() {
                        moved += 1;
                        let row = rows.remove(from);
                        let to = to.min(rows.len());
                        trace_list_event::<C>(
                            container,
                            format_args!("move id={} from={} to={}", row.id, from, to),
                        );
                        rows.insert(to, row);
                    }
                }
            }
        }

        let reused = items.len().saturating_sub(created);
        reconcile_widget_order(container, &rows);

        tracing::trace!(
            previous = previous_len,
            next = items.len(),
            reused,
            created,
            removed,
            moved,
            "list reconciled"
        );
        trace_list_lifecycle::<C>(
            container,
            previous_len,
            items.len(),
            reused,
            created,
            removed,
        );
        self.rows = rows;
    }
}

fn reconcile_widget_order<C>(container: &gtk::Box, rows: &[ComponentListRow<C>])
where
    C: Component,
    C::Init: Clone,
    C::Root: AsRef<gtk::Widget>,
{
    let mut previous_widget = None;
    let mut previous_id = None;

    for (index, row) in rows.iter().enumerate() {
        let widget = row.widget();
        if widget.parent().is_none() {
            trace_list_event::<C>(
                container,
                format_args!("append id={} index={}", row.id, index),
            );
            container.append(widget);
        }

        if widget.prev_sibling() != previous_widget {
            trace_list_event::<C>(
                container,
                format_args!(
                    "reorder id={} index={} after={previous_id:?}",
                    row.id, index
                ),
            );
            container.reorder_child_after(widget, previous_widget.as_ref());
        }

        previous_widget = Some(widget.clone());
        previous_id = Some(row.id);
    }
}

struct ComponentListRow<C>
where
    C: Component,
{
    id: u64,
    item: C::Init,
    controller: Controller<C>,
}

impl<C> ComponentListRow<C>
where
    C: Component,
    C::Init: Clone,
{
    fn new(item: C::Init, id: u64) -> Self {
        let controller = C::builder().launch(item.clone()).detach();
        Self {
            id,
            item,
            controller,
        }
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
fn component_list_host<C>(container: &gtk::Box, key: Quark) -> &mut ComponentListHost<C>
where
    C: Component,
{
    // GTK object data owns the row controllers for this container. The quark is
    // derived from the row component type, so the downcast type matches writes.
    unsafe {
        if container.qdata::<ComponentListHost<C>>(key).is_none() {
            container.set_qdata(key, ComponentListHost::<C>::default());
        }

        let host: NonNull<ComponentListHost<C>> = container
            .qdata(key)
            .expect("component list host was just installed");
        host.as_ptr()
            .as_mut()
            .expect("component list host pointer must be valid")
    }
}

fn trace_list_lifecycle<C>(
    container: &gtk::Box,
    previous_len: usize,
    next_len: usize,
    reused: usize,
    created: usize,
    removed: usize,
) where
    C: Component,
{
    if !list_trace_enabled() {
        return;
    }

    eprintln!(
        "[shell-core/list] row={} container={} previous={} next={} reused={} created={} removed={}",
        std::any::type_name::<C>(),
        container.widget_name(),
        previous_len,
        next_len,
        reused,
        created,
        removed
    );
}

fn trace_list_event<C>(container: &gtk::Box, message: std::fmt::Arguments<'_>)
where
    C: Component,
{
    if !list_trace_enabled() {
        return;
    }

    eprintln!(
        "[shell-core/list] row={} container={} {message}",
        std::any::type_name::<C>(),
        container.widget_name()
    );
}

fn list_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("SHELL_CORE_LIST_TRACE").is_some())
}
