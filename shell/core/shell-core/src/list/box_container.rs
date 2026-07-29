use std::{ptr::NonNull, sync::OnceLock};

use gtk::glib::Quark;
use gtk::prelude::{BoxExt, Cast, IsA, ObjectExt, WidgetExt};
use relm4::component::ComponentController;
use relm4::{Component, Controller};

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
        let container = self.upcast_ref::<gtk::Box>();
        let key = component_list_key::<C>();
        let host = component_list_host::<C>(container, key);
        host.reconcile(container, update.items);
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
    C::Init: Clone + PartialEq + 'static,
    C::Root: AsRef<gtk::Widget> + Clone + std::fmt::Debug,
{
    fn reconcile(&mut self, container: &gtk::Box, items: &[C::Init]) {
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

        let mut reused = 0usize;
        let mut created = 0usize;

        let mut old_rows = std::mem::take(&mut self.rows);
        let mut rows = Vec::with_capacity(items.len());

        for (new_index, item) in items.iter().enumerate() {
            let row = match old_rows.iter().position(|row| &row.item == item) {
                Some(old_index) => {
                    reused += 1;
                    let row = old_rows.remove(old_index);
                    trace_list_event::<C>(
                        container,
                        format_args!(
                            "reuse id={} old_index={} new_index={}",
                            row.id, old_index, new_index
                        ),
                    );
                    row
                }
                None => {
                    created += 1;
                    let id = self.next_row_id;
                    self.next_row_id += 1;
                    trace_list_event::<C>(
                        container,
                        format_args!("create id={} new_index={}", id, new_index),
                    );
                    ComponentListRow::new(item.clone(), id)
                }
            };
            rows.push(row);
        }

        let removed = old_rows.len();
        for row in &old_rows {
            trace_list_event::<C>(container, format_args!("remove id={}", row.id));
            container.remove(row.widget());
        }

        reconcile_widget_order(container, &rows);

        tracing::trace!(
            previous = previous_len,
            next = items.len(),
            reused,
            created,
            removed,
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
