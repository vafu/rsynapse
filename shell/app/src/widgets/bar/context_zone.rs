use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::*},
    list::ComponentListBoxExt,
    source::{Observable, rx::Observable as _},
};

use super::bzbus;
use crate::widgets::nerd_icon::{NerdIcon, NerdIconLabelExt};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ContextItem {
    Build(bzbus::BzBusView),
}

#[shell_macros::model(module = context_zone_sources)]
pub(super) struct ContextZone {
    #[source(context_items())]
    items: Vec<ContextItem>,
}

#[shell_macros::component(
    module = context_zone_sources,
    model = ContextZone
)]
#[relm4::component(pub(crate))]
impl SimpleComponent for ContextZone {
    type Init = ();
    type Input = context_zone_sources::Msg;
    type Output = ();

    view! {
        #[root]
        gtk::Revealer {
            add_css_class: "context-zone-revealer",
            #[watch]
            set_reveal_child: !model.items.is_empty(),
            set_transition_type: gtk::RevealerTransitionType::FadeSlideDown,
            set_transition_duration: 140,
            set_halign: gtk::Align::Center,
            set_valign: gtk::Align::Start,

            gtk::Box {
                add_css_class: "context-zone",
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Start,
                set_orientation: gtk::Orientation::Vertical,

                #[bind_list(items, row = ContextZoneItem)]
                items -> gtk::Box {
                    add_css_class: "bar-indicator-list-vertical",
                    add_css_class: "context-zone-list",
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Start,
                    set_orientation: gtk::Orientation::Vertical,
                }
            }
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = ContextZone::new();
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }
}

#[derive(Debug)]
pub(super) struct ContextZoneItem {
    item: ContextItem,
}

#[relm4::component(pub(crate))]
impl SimpleComponent for ContextZoneItem {
    type Init = ContextItem;
    type Input = ();
    type Output = ();

    view! {
        #[root]
        gtk::Revealer {
            set_reveal_child: true,
            set_transition_type: gtk::RevealerTransitionType::FadeSlideDown,
            set_transition_duration: 140,
            set_halign: gtk::Align::Center,
            set_valign: gtk::Align::Center,

            gtk::Overlay {
                set_css_classes: &item_classes(&model.item),
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Center,
                set_tooltip_text: Some(item_tooltip(&model.item).as_str()),

                gtk::Label {
                    set_css_classes: &["bar-indicator-icon", "nerdicon"],
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    set_nerd_icon: item_icon(&model.item),
                },

                add_overlay = &gtk::DrawingArea {
                    set_visible: item_progress_visible(&model.item),
                    set_css_classes: bzbus::progress_track_classes(),
                    set_halign: gtk::Align::Fill,
                    set_valign: gtk::Align::Fill,
                    set_hexpand: true,
                    set_vexpand: true,
                    set_can_target: false,
                    set_draw_func: bzbus::progress_track_draw_func(),
                },

                add_overlay = &gtk::DrawingArea {
                    set_visible: item_progress_visible(&model.item),
                    set_css_classes: &item_progress_level_classes(&model.item),
                    set_halign: gtk::Align::Fill,
                    set_valign: gtk::Align::Fill,
                    set_hexpand: true,
                    set_vexpand: true,
                    set_can_target: false,
                    set_draw_func: bzbus::progress_level_draw_func(item_progress_percent(&model.item)),
                }
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = ContextZoneItem { item: init };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }
}

fn context_items() -> Observable<Vec<ContextItem>> {
    bzbus::ongoing_builds()
        .map(|builds| builds.into_iter().map(ContextItem::Build).collect())
        .distinct_until_changed()
        .box_it()
}

fn item_classes(item: &ContextItem) -> Vec<&'static str> {
    let mut classes = vec!["bar-indicator", "context-zone-item"];
    match item {
        ContextItem::Build(build) => {
            classes.push("context-zone-build");
            if build.progress_visible {
                classes.push("build-progress");
            }
            classes.extend(build_state_classes(build));
        }
    }
    classes
}

fn build_state_classes(build: &bzbus::BzBusView) -> impl Iterator<Item = &'static str> + '_ {
    build.classes.iter().copied().filter(|class| {
        matches!(
            *class,
            "idle" | "offline" | "running" | "failed" | "finished"
        )
    })
}

fn item_tooltip(item: &ContextItem) -> String {
    match item {
        ContextItem::Build(build) => format!("Build\n{}", build.tooltip),
    }
}

fn item_icon(item: &ContextItem) -> NerdIcon {
    match item {
        ContextItem::Build(build) => build.icon.clone(),
    }
}

fn item_progress_visible(item: &ContextItem) -> bool {
    match item {
        ContextItem::Build(build) => build.progress_visible,
    }
}

fn item_progress_percent(item: &ContextItem) -> u8 {
    match item {
        ContextItem::Build(build) => build.progress_percent,
    }
}

fn item_progress_level_classes(item: &ContextItem) -> Vec<&'static str> {
    match item {
        ContextItem::Build(build) => build.progress_level_classes.clone(),
    }
}
