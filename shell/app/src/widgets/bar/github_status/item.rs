use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::*},
    list::{ComponentListBoxExt, ComponentListUpdate},
};

use super::super::bar_item;
use super::{
    Scope,
    category::Category,
    row::PullRequestRow,
    source::{self, GitHubView},
};
use crate::widgets::nerd_icon::NerdIconLabelExt;

#[shell_macros::model(module = github_item_sources)]
pub(super) struct GitHubItem {
    scope: Scope,
    category: Category,
    #[source(source::status(scope.clone(), *category))]
    status: GitHubView,
}

#[shell_macros::component(module = github_item_sources, model = GitHubItem)]
#[relm4::component(pub(crate))]
impl SimpleComponent for GitHubItem {
    type Init = (Scope, Category);
    type Input = github_item_sources::Msg;
    type Output = ();

    view! {
        #[root]
        gtk::Box {
            set_css_classes: &bar_item::classes(&[bar_item::SQUARE_CLASS, "github-status-item"]),
            set_orientation: gtk::Orientation::Horizontal,
            set_halign: gtk::Align::Center,
            set_valign: gtk::Align::Center,
            #[watch]
            set_visible: model.status.visible(),
            #[watch]
            set_tooltip_text: Some(model.status.tooltip(model.category).as_str()),
            #[watch]
            set_opacity: if model.status.available { 1.0 } else { 0.55 },

            gtk::MenuButton {
                set_css_classes: &bar_item::action_classes(&["github-status-button"]),
                set_direction: gtk::ArrowType::Right,

                #[wrap(Some)]
                set_child = &gtk::Overlay {
                    gtk::Label {
                        set_css_classes: &["nerdicon", bar_item::ICON_CLASS, model.category.css_class()],
                        set_nerd_icon: model.category.icon(),
                    },
                    add_overlay = &gtk::Label {
                        set_css_classes: &["bar-badge", "bar-count-badge"],
                        set_halign: gtk::Align::End,
                        set_valign: gtk::Align::End,
                        #[watch]
                        set_label: &model.status.count(),
                    },
                    #[local_ref]
                    add_overlay = &unseen_badge -> gtk::Box {
                        #[watch]
                        set_visible: model.status.unseen_count() > 0,
                    },
                },

                #[wrap(Some)]
                set_popover = &gtk::Popover {
                    add_css_class: "github-pr-popover",
                    set_position: gtk::PositionType::Right,
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 8,
                        set_margin_all: 12,
                        gtk::Label {
                            set_label: model.category.title(),
                            add_css_class: "heading",
                            set_xalign: 0.0,
                        },
                        gtk::Label {
                            set_xalign: 0.0,
                            #[watch]
                            set_visible: model.status.rows.is_empty(),
                            #[watch]
                            set_label: model.status.empty_message(),
                        },
                        gtk::ScrolledWindow {
                            set_hscrollbar_policy: gtk::PolicyType::Never,
                            set_vscrollbar_policy: gtk::PolicyType::Automatic,
                            set_propagate_natural_width: true,
                            set_propagate_natural_height: true,
                            #[watch]
                            set_visible: !model.status.rows.is_empty(),
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 4,
                                #[watch]
                                set_component_list: ComponentListUpdate::<PullRequestRow>::new(&model.status.rows),
                            },
                        },
                    },
                },
            },
        }
    }

    fn init(
        (scope, category): Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = GitHubItem::new(scope, category);
        let unseen_badge = bar_item::unseen_badge();
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }
}
