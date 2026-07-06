mod box_container;

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
}
