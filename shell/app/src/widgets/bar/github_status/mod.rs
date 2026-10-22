mod actions;
mod category;
mod item;
mod launch;
mod row;
mod scope;
mod source;

use relm4::prelude::*;
use shell_core::gtk::{self, prelude::*};

use super::bar_item;
use crate::widgets::button_group::{ButtonGroup, Config, Direction, RevealMode};
use crate::widgets::nerd_icon::{NerdIcon, NerdIconLabelExt, cod};

use category::Category;
use item::GitHubItem;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct Scope {
    pub(super) output_name: Option<String>,
}

pub(super) struct GitHubStatus {
    _items: [Controller<GitHubItem>; Category::ALL.len()],
}

#[relm4::component(pub(crate))]
impl SimpleComponent for GitHubStatus {
    type Init = Scope;
    type Input = ();
    type Output = ();

    view! {
        #[root]
        ButtonGroup {
            set_widget_name: "github-status",
            add_css_class: "button-group-pill",
            add_css_class: "github-status",
            set_config: Config { direction: Direction::Down, reveal_mode: RevealMode::Always },
            set_halign: gtk::Align::Center,
            set_valign: gtk::Align::Start,

            set_owner = &gtk::Box {
                set_css_classes: &bar_item::classes(&[
                    bar_item::SQUARE_CLASS,
                ]),
                set_tooltip_text: Some("GitHub pull requests"),
                gtk::Label {
                    set_css_classes: &["nerdicon", bar_item::ICON_CLASS],
                    set_hexpand: true,
                    set_nerd_icon: NerdIcon::new(cod::COD_GITHUB),
                },
            },

            set_content = &gtk::Box {
                    set_spacing: 4,

                    #[local_ref]
                    merging_root -> gtk::Box {},
                    #[local_ref]
                    merged_root -> gtk::Box {},
                    #[local_ref]
                    failed_root -> gtk::Box {},
                    #[local_ref]
                    pending_root -> gtk::Box {},
                    #[local_ref]
                    approved_root -> gtk::Box {},
                    #[local_ref]
                    to_address_root -> gtk::Box {},
                    #[local_ref]
                    conflicts_root -> gtk::Box {},
                    #[local_ref]
                    changes_root -> gtk::Box {},
                    #[local_ref]
                    review_root -> gtk::Box {},
                    #[local_ref]
                    drafts_root -> gtk::Box {},
            },
        }
    }

    fn init(
        scope: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let items = Category::ALL.map(|category| {
            GitHubItem::builder()
                .launch((scope.clone(), category))
                .detach()
        });
        let [
            merging_root,
            merged_root,
            failed_root,
            pending_root,
            approved_root,
            to_address_root,
            conflicts_root,
            changes_root,
            review_root,
            drafts_root,
        ] = items.each_ref().map(|item| item.widget().clone());
        let model = GitHubStatus { _items: items };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }
}
