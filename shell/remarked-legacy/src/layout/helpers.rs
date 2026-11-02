use crate::WorkspaceInput;
use adw::prelude::*;
use relm4::Sender;

pub(super) fn list() -> gtk::ListBox {
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("navigation-sidebar");
    list
}
pub(super) fn scroll(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(child)
        .build()
}
pub(super) fn button(
    icon: &str,
    tooltip: &str,
    sender: Sender<WorkspaceInput>,
    message: impl Fn() -> WorkspaceInput + 'static,
) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_tooltip_text(Some(tooltip));
    button.connect_clicked(move |_| sender.emit(message()));
    button
}
