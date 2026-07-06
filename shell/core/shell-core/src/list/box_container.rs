use std::{ptr::NonNull, sync::OnceLock};

use gtk::glib::Quark;
use gtk::prelude::{BoxExt, Cast, IsA, ObjectExt};
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
}

impl<C> Default for ComponentListHost<C>
where
    C: Component,
{
    fn default() -> Self {
        Self { rows: Vec::new() }
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
            trace_list_lifecycle::<C>(previous_len, items.len(), items.len(), 0, 0);
            return;
        }

        let mut reused = 0usize;
        let mut created = 0usize;

        for row in &self.rows {
            container.remove(row.widget());
        }

        let mut old_rows = std::mem::take(&mut self.rows);
        let mut rows = Vec::with_capacity(items.len());

        for item in items {
            let row = old_rows
                .iter()
                .position(|row| &row.item == item)
                .map(|index| {
                    reused += 1;
                    old_rows.remove(index)
                })
                .unwrap_or_else(|| {
                    created += 1;
                    ComponentListRow::new(item.clone())
                });
            container.append(row.widget());
            rows.push(row);
        }

        let removed = old_rows.len();
        tracing::trace!(
            previous = previous_len,
            next = items.len(),
            reused,
            created,
            removed,
            "list reconciled"
        );
        trace_list_lifecycle::<C>(previous_len, items.len(), reused, created, removed);
        self.rows = rows;
    }
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
    fn new(item: C::Init) -> Self {
        let controller = C::builder().launch(item.clone()).detach();
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
    previous_len: usize,
    next_len: usize,
    reused: usize,
    created: usize,
    removed: usize,
) where
    C: Component,
{
    static ENABLED: OnceLock<bool> = OnceLock::new();
    if !*ENABLED.get_or_init(|| std::env::var_os("SHELL_CORE_LIST_TRACE").is_some()) {
        return;
    }

    eprintln!(
        "[shell-core/list] reconcile row={} previous={} next={} reused={} created={} removed={}",
        std::any::type_name::<C>(),
        previous_len,
        next_len,
        reused,
        created,
        removed
    );
}
