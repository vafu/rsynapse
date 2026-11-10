use data_sources as source;
use data_sources::systray::{TrayMenu, TrayMenuItem, TrayMenuKind, TrayToggle};
use relm4::prelude::*;
use shell_core::gtk::{self, prelude::*};

use super::{TrayItem, TrayItemInput, icon};

const MENU_ICON_SIZE: i32 = 16;

pub(super) fn rebuild(menu: &TrayMenu, container: &gtk::Box, sender: &ComponentSender<TrayItem>) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    let visible = menu
        .items
        .iter()
        .filter(|item| item.is_visible())
        .collect::<Vec<_>>();
    if visible.is_empty() {
        let label = gtk::Label::new(Some("No menu items"));
        label.add_css_class("dim-label");
        label.set_xalign(0.0);
        container.append(&label);
        return;
    }

    for item in visible {
        append_item(container, menu, item, sender);
    }
}

fn append_item(
    container: &gtk::Box,
    menu: &TrayMenu,
    item: &TrayMenuItem,
    sender: &ComponentSender<TrayItem>,
) {
    match item.kind {
        TrayMenuKind::Separator => {
            let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
            separator.add_css_class("tray-menu-separator");
            container.append(&separator);
        }
        TrayMenuKind::Submenu => append_submenu(container, menu, item, sender),
        TrayMenuKind::Standard => {
            let button = gtk::Button::new();
            button.set_has_frame(false);
            button.add_css_class("tray-menu-row");
            button.set_sensitive(item.enabled);
            button.set_child(Some(&row_content(item, false)));
            let sender = sender.clone();
            let id = item.id;
            button.connect_clicked(move |_| sender.input(TrayItemInput::MenuEvent(id)));
            container.append(&button);
        }
    }
}

fn append_submenu(
    container: &gtk::Box,
    menu: &TrayMenu,
    item: &TrayMenuItem,
    sender: &ComponentSender<TrayItem>,
) {
    let children = item
        .children
        .iter()
        .filter(|child| child.is_visible())
        .collect::<Vec<_>>();
    if children.is_empty() && !item.children.is_empty() {
        return;
    }

    let button = gtk::MenuButton::new();
    button.set_has_frame(false);
    button.add_css_class("tray-menu-row");
    button.set_sensitive(item.enabled);
    button.set_child(Some(&row_content(item, true)));

    let popover = gtk::Popover::new();
    popover.add_css_class("tray-menu-popover");
    popover.set_has_arrow(true);
    popover.set_position(gtk::PositionType::Right);
    let submenu = gtk::Box::new(gtk::Orientation::Vertical, 2);
    submenu.add_css_class("tray-menu");
    submenu.add_css_class("tray-submenu");
    submenu.set_margin_all(6);
    popover.set_child(Some(&submenu));
    for child in children {
        append_item(&submenu, menu, child, sender);
    }

    let menu_for_show = menu.clone();
    let id = item.id;
    popover.connect_show(move |_| source::systray::menu_about_to_show(&menu_for_show, id));
    button.set_popover(Some(&popover));
    container.append(&button);
}

fn row_content(item: &TrayMenuItem, has_submenu: bool) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.set_margin_start(6);
    row.set_margin_end(6);
    row.set_margin_top(2);
    row.set_margin_bottom(2);

    match item.toggle {
        TrayToggle::Check(true) | TrayToggle::Radio(true) => {
            let check = gtk::Image::from_icon_name("object-select-symbolic");
            check.add_css_class("tray-menu-indicator");
            row.append(&check);
        }
        TrayToggle::Check(false) | TrayToggle::Radio(false) => {
            let placeholder = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            placeholder.add_css_class("tray-menu-indicator");
            placeholder.set_size_request(MENU_ICON_SIZE, -1);
            row.append(&placeholder);
        }
        TrayToggle::None => {}
    }

    if !matches!(item.icon, data_sources::systray::TrayIcon::None) {
        row.append(&icon::image(&item.icon, MENU_ICON_SIZE));
    }

    let label = gtk::Label::new(Some(&item.label));
    label.set_use_underline(true);
    label.set_xalign(0.0);
    label.set_hexpand(true);
    row.append(&label);

    if has_submenu {
        row.append(&gtk::Image::from_icon_name("pan-end-symbolic"));
    }

    row
}
