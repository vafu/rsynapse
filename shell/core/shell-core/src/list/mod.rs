mod box_container;

#[cfg(test)]
mod test;

use gtk::prelude::*;
use relm4::Component;

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
        C::Init: Clone + 'static,
        K: Eq + std::hash::Hash + 'static,
        F: Fn(&C::Init) -> K + 'static,
        C::Root: AsRef<gtk::Widget> + Clone + std::fmt::Debug;
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
