use super::*;

pub(super) fn mount_popover_component<C>(
    popover: &gtk::Popover,
    mount: &gtk::Box,
    controller: &Rc<RefCell<Option<Controller<C>>>>,
    init: C::Init,
) where
    C: Component,
    C::Init: Clone,
    C::Root: AsRef<gtk::Widget> + Clone + std::fmt::Debug,
{
    if popover.is_visible() {
        if controller.borrow().is_none() {
            let launched = C::builder().launch(init).detach();
            let widget = <C::Root as AsRef<gtk::Widget>>::as_ref(launched.widget()).clone();
            mount.append(&widget);
            *controller.borrow_mut() = Some(launched);
        }
        return;
    }

    if let Some(launched) = controller.borrow_mut().take() {
        let widget = <C::Root as AsRef<gtk::Widget>>::as_ref(launched.widget()).clone();
        mount.remove(&widget);
    }
}

pub(super) fn mount_bluetooth_group_popover(popover: &gtk::Popover, group: BluetoothDeviceGroup) {
    let mount = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    popover.set_child(Some(&mount));
    let controller = Rc::new(RefCell::new(None));
    mount_popover_component::<BluetoothGroupPopover>(popover, &mount, &controller, group);

    let popover_for_signal = popover.clone();
    let popover_for_mount = popover.clone();
    let mount_for_mount = mount.clone();
    popover_for_signal.connect_visible_notify(move |_| {
        mount_popover_component::<BluetoothGroupPopover>(
            &popover_for_mount,
            &mount_for_mount,
            &controller,
            group,
        );
    });
}
