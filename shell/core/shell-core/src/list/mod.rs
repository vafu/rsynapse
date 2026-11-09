mod box_container;
mod list_box_container;

#[cfg(test)]
mod test;

use gtk::prelude::*;
use relm4::{Component, Sender};

pub struct ComponentListUpdate<'a, C>
where
    C: Component,
{
    items: &'a [C::Init],
}

impl<'a, C> ComponentListUpdate<'a, C>
where
    C: Component,
{
    pub fn new(items: &'a [C::Init]) -> Self {
        Self { items }
    }
}

pub trait ComponentListBoxExt {
    /// Reconciles child Relm4 row components into this widget.
    ///
    /// `#[bind_list(...)]` emits a call to this method. If the annotated widget
    /// type has no list extension with this method in scope, Rust reports the
    /// unsupported backend at compile time.
    fn set_component_list<C>(&self, update: ComponentListUpdate<'_, C>)
    where
        C: Component,
        C::Init: Clone + PartialEq + 'static,
        C::Root: AsRef<gtk::Widget> + Clone + std::fmt::Debug;

    /// Reconciles rows using a stable key instead of item equality.
    fn set_component_list_by_key<C, K, F>(&self, update: ComponentListUpdate<'_, C>, key: F)
    where
        C: Component,
        C::Init: Clone + PartialEq + 'static,
        K: Eq + std::hash::Hash + 'static,
        F: Fn(&C::Init) -> K + 'static,
        C::Root: AsRef<gtk::Widget> + Clone + std::fmt::Debug;

    /// Reconciles child components and forwards their outputs to `output`.
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
        F: Fn(C::Output) -> O + Clone + 'static;

    /// Reconciles keyed child components and forwards their outputs to `output`.
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
        M: Fn(C::Output) -> O + Clone + 'static;
}

/// Scrolls `scrolled` so `row` is fully visible.
pub fn scroll_row_into_view(scrolled: &gtk::ScrolledWindow, row: &gtk::ListBoxRow) {
    let Some(bounds) = row.compute_bounds(scrolled) else {
        return;
    };
    let adjustment = scrolled.vadjustment();
    let top = bounds.y() as f64;
    let bottom = top + bounds.height() as f64;
    if top < adjustment.value() {
        adjustment.set_value(top);
    } else if bottom > adjustment.value() + adjustment.page_size() {
        adjustment.set_value(bottom - adjustment.page_size());
    }
}
