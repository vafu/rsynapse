mod icon;
mod menu;

use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::*},
    source::{self, Observable},
};

use source::systray::{TrayItem as TrayItemSnapshot, TrayItemKey, TrayMenu, TrayStatus};

const TRAY_ICON_SIZE: i32 = 18;

pub(crate) type TrayItemNode = TrayItemKey;

pub(crate) fn systray_items() -> Observable<Vec<TrayItemNode>> {
    source::systray::item_keys()
}

#[derive(Debug)]
pub(crate) enum TrayItemInput {
    Source(Box<tray_item_sources::Msg>),
    Primary,
    Secondary,
    MenuEvent(i32),
}

impl From<tray_item_sources::Msg> for TrayItemInput {
    fn from(message: tray_item_sources::Msg) -> Self {
        Self::Source(Box::new(message))
    }
}

#[shell_macros::model(module = tray_item_sources)]
pub(crate) struct TrayItem {
    item: TrayItemNode,
    icon_widget: gtk::Image,
    popover: gtk::Popover,
    menu_box: gtk::Box,

    #[source(source::systray::item(item.clone()))]
    view: TrayItemSnapshot,

    #[source(source::systray::menu(item.clone()))]
    menu: TrayMenu,
}

#[shell_macros::component(module = tray_item_sources, model = TrayItem)]
#[relm4::component(pub(crate))]
impl SimpleComponent for TrayItem {
    type Init = TrayItemNode;
    type Input = TrayItemInput;
    type Output = ();

    view! {
        #[root]
        gtk::Button {
            #[watch]
            set_css_classes: &tray_button_classes(&model.view),
            set_has_frame: false,
            #[watch]
            set_tooltip_text: Some(model.view.tooltip_text().as_str()),
            connect_clicked => TrayItemInput::Primary,

            #[wrap(Some)]
            set_child = &gtk::Image {
                set_pixel_size: TRAY_ICON_SIZE,
            },
        }
    }

    fn init(
        item: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let popover = gtk::Popover::new();
        popover.add_css_class("tray-menu-popover");
        popover.set_has_arrow(true);
        popover.set_position(gtk::PositionType::Top);
        popover.set_parent(&root);
        let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        menu_box.add_css_class("tray-menu");
        menu_box.set_margin_all(6);
        popover.set_child(Some(&menu_box));

        let secondary = gtk::GestureClick::new();
        secondary.set_button(gtk::gdk::BUTTON_SECONDARY);
        secondary.connect_released({
            let sender = sender.clone();
            move |_, _, _, _| sender.input(TrayItemInput::Secondary)
        });
        root.add_controller(secondary);

        let mut model = TrayItem::new(item, gtk::Image::new(), popover, menu_box);
        let widgets = view_output!();
        model.icon_widget = root
            .child()
            .and_then(|child| child.downcast::<gtk::Image>().ok())
            .expect("tray button child should be an image");
        register_icon_theme_path(&model.view);
        icon::apply(&model.icon_widget, model.view.active_icon(), TRAY_ICON_SIZE);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Self::Input, sender: ComponentSender<Self>) {
        match input {
            TrayItemInput::Source(message) => {
                let previous_icon = self.view.active_icon().clone();
                let previous_menu = self.menu.clone();
                TrayItem::update(self, *message);
                register_icon_theme_path(&self.view);
                if self.view.active_icon() != &previous_icon {
                    icon::apply(&self.icon_widget, self.view.active_icon(), TRAY_ICON_SIZE);
                }
                if self.menu != previous_menu && self.popover.is_visible() {
                    menu::rebuild(&self.menu, &self.menu_box, &sender);
                }
            }
            TrayItemInput::Primary => {
                if self.view.has_menu() {
                    self.show_menu(&sender);
                } else {
                    source::systray::activate(&self.item);
                }
            }
            TrayItemInput::Secondary => {
                if self.view.has_menu() {
                    self.show_menu(&sender);
                } else {
                    source::systray::secondary_activate(&self.item);
                }
            }
            TrayItemInput::MenuEvent(id) => {
                source::systray::menu_event(&self.menu, id, "clicked");
                self.popover.popdown();
            }
        }
    }

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        self.popover.unparent();
    }
}

impl TrayItem {
    fn show_menu(&self, sender: &ComponentSender<Self>) {
        source::systray::menu_about_to_show(&self.menu, 0);
        menu::rebuild(&self.menu, &self.menu_box, sender);
        self.popover.popup();
    }
}

fn register_icon_theme_path(view: &TrayItemSnapshot) {
    if let Some(path) = &view.icon_theme_path {
        icon::register_theme_path(path);
    }
}

fn tray_button_classes(view: &TrayItemSnapshot) -> Vec<&'static str> {
    let mut classes = vec!["flat", "bar-action", "tray-item"];
    match view.status {
        TrayStatus::NeedsAttention => classes.push("needs-attention"),
        TrayStatus::Passive => classes.push("passive"),
        TrayStatus::Active => {}
    }
    classes
}
