use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::*},
    list::ComponentListBoxExt,
    source::{Observable, rx::Observable as _},
};

use super::bzbus::{self, BzBusView};
use crate::widgets::nerd_icon::{NerdIcon, NerdIconLabelExt};

#[shell_macros::model(module = context_zone_sources)]
pub(super) struct ContextZone {
    #[source(context_items())]
    items: Vec<ContextItemInit>,
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
            set_valign: gtk::Align::Center,

            gtk::Box {
                add_css_class: "context-zone",
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Center,
                set_orientation: gtk::Orientation::Vertical,

                #[bind_list(items, row = ContextZoneItem)]
                items -> gtk::Box {
                    add_css_class: "bar-indicator-list-vertical",
                    add_css_class: "context-zone-list",
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ContextItemInit {
    id: String,
}

impl From<String> for ContextItemInit {
    fn from(id: String) -> Self {
        Self { id }
    }
}

#[derive(Debug)]
#[shell_macros::model(module = context_zone_item_sources)]
pub(super) struct ContextZoneItem {
    id: String,

    #[source(build_view(id.clone()))]
    build: Option<BzBusView>,
}

#[shell_macros::component(
    module = context_zone_item_sources,
    model = ContextZoneItem
)]
#[relm4::component(pub(crate))]
impl SimpleComponent for ContextZoneItem {
    type Init = ContextItemInit;
    type Input = context_zone_item_sources::Msg;
    type Output = ();

    view! {
        #[root]
        gtk::Revealer {
            #[watch]
            set_reveal_child: model.build.is_some(),
            set_transition_type: gtk::RevealerTransitionType::FadeSlideDown,
            set_transition_duration: 140,
            set_halign: gtk::Align::Center,
            set_valign: gtk::Align::Center,

            gtk::Overlay {
                #[watch]
                set_css_classes: &item_classes(&model.build),
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Center,
                #[watch]
                set_tooltip_text: item_tooltip(&model.build).as_deref(),

                gtk::Label {
                    set_css_classes: &["bar-indicator-icon", "nerdicon"],
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    #[watch]
                    set_nerd_icon: item_icon(&model.build),
                },

                add_overlay = &gtk::DrawingArea {
                    #[watch]
                    set_visible: item_progress_visible(&model.build),
                    set_css_classes: bzbus::progress_track_classes(),
                    set_halign: gtk::Align::Fill,
                    set_valign: gtk::Align::Fill,
                    set_hexpand: true,
                    set_vexpand: true,
                    set_can_target: false,
                    set_draw_func: bzbus::progress_track_draw_func(),
                },

                add_overlay = &gtk::DrawingArea {
                    #[watch]
                    set_visible: item_progress_visible(&model.build),
                    #[watch]
                    set_css_classes: &item_progress_level_classes(&model.build),
                    set_halign: gtk::Align::Fill,
                    set_valign: gtk::Align::Fill,
                    set_hexpand: true,
                    set_vexpand: true,
                    set_can_target: false,
                    #[watch]
                    set_draw_func: bzbus::progress_level_draw_func(item_progress_percent(&model.build)),
                }
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = ContextZoneItem::new(init.id);
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }
}

fn context_items() -> Observable<Vec<ContextItemInit>> {
    bzbus::ongoing_build_ids()
        .map(context_items_from_ids)
        .distinct_until_changed()
        .box_it()
}

fn context_items_from_ids(ids: Vec<String>) -> Vec<ContextItemInit> {
    ids.into_iter().map(ContextItemInit::from).collect()
}

fn build_view(id: String) -> Observable<Option<BzBusView>> {
    bzbus::build_view(id)
}

fn item_classes(build: &Option<BzBusView>) -> Vec<&'static str> {
    let mut classes = vec!["bar-indicator", "context-zone-item", "context-zone-build"];
    let Some(build) = build else {
        classes.push("offline");
        return classes;
    };
    if build.progress_visible {
        classes.push("build-progress");
    }
    classes.extend(build_state_classes(build));
    classes
}

fn build_state_classes(build: &BzBusView) -> impl Iterator<Item = &'static str> + '_ {
    build.classes.iter().copied().filter(|class| {
        matches!(
            *class,
            "idle" | "offline" | "running" | "failed" | "finished"
        )
    })
}

fn item_tooltip(build: &Option<BzBusView>) -> Option<String> {
    build
        .as_ref()
        .map(|build| format!("Build\n{}", build.tooltip))
}

fn item_icon(build: &Option<BzBusView>) -> NerdIcon {
    build
        .as_ref()
        .map(|build| build.icon.clone())
        .unwrap_or_else(|| NerdIcon::new(crate::widgets::nerd_icon::md::MD_CLOUD_OFF_OUTLINE))
}

fn item_progress_visible(build: &Option<BzBusView>) -> bool {
    build.as_ref().is_some_and(|build| build.progress_visible)
}

fn item_progress_percent(build: &Option<BzBusView>) -> u8 {
    build.as_ref().map_or(0, |build| build.progress_percent)
}

fn item_progress_level_classes(build: &Option<BzBusView>) -> Vec<&'static str> {
    build
        .as_ref()
        .map(|build| build.progress_level_classes.clone())
        .unwrap_or_else(|| vec!["level", "offline"])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_item_init_equality_ignores_build_view_changes() {
        let running = context_items_from_ids(vec!["invocation-1".to_owned()]);
        let later = context_items_from_ids(vec!["invocation-1".to_owned()]);

        assert_eq!(running, later);
    }
}
