use shell_core::gtk::{self, prelude::*};

pub(super) const SIZE: i32 = 28;

pub(super) trait BarIndicatorExt {
    fn set_bar_indicator_size(&self, size: i32);
}

impl<T: IsA<gtk::Widget>> BarIndicatorExt for T {
    fn set_bar_indicator_size(&self, size: i32) {
        self.set_size_request(size, size);
        self.set_hexpand(false);
        self.set_vexpand(false);
        self.set_halign(gtk::Align::Center);
        self.set_valign(gtk::Align::Center);
    }
}
