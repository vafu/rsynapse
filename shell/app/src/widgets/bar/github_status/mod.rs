mod actions;
mod category;
mod item;
mod launch;
mod row;
mod scope;
mod source;
#[cfg(test)]
mod test;

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
        gtk::Box {
            set_widget_name: "github-status",
            add_css_class: "github-status",
            set_orientation: gtk::Orientation::Vertical,
            set_halign: gtk::Align::Center,
            set_valign: gtk::Align::Start,

            ButtonGroup {
                add_css_class: "button-group-pill",
                add_css_class: "github-authored-block",
                set_config: Config { direction: Direction::Down, reveal_mode: RevealMode::Always },

                #[local_ref]
                set_owner = &authored_owner -> gtk::Box {},

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
            },

            #[name = "activity_group"]
            ButtonGroup {
                add_css_class: "button-group-pill",
                add_css_class: "github-activity-block",
                set_config: Config { direction: Direction::Down, reveal_mode: RevealMode::Always },

                #[local_ref]
                set_owner = &activity_owner -> gtk::Box {},

                set_content = &gtk::Box {
                    set_spacing: 4,
                    #[local_ref]
                    initial_root -> gtk::Box {},
                    #[local_ref]
                    rereview_root -> gtk::Box {},
                    #[local_ref]
                    my_changes_root -> gtk::Box {},
                },
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
            initial_root,
            rereview_root,
            my_changes_root,
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
        let authored_owner = block_owner(
            NerdIcon::new(cod::COD_GIT_PULL_REQUEST),
            "Your pull requests",
        );
        let activity_owner = block_owner(NerdIcon::new(cod::COD_GITHUB), "GitHub activity");
        let model = GitHubStatus { _items: items };
        let widgets = view_output!();
        show_when_any_visible(
            &widgets.activity_group,
            &[initial_root, rereview_root, my_changes_root],
        );
        ComponentParts { model, widgets }
    }
}

fn show_when_any_visible(group: &ButtonGroup, items: &[gtk::Box]) {
    let weak_group = group.downgrade();
    let weak_items: Vec<_> = items.iter().map(|item| item.downgrade()).collect();
    let update = std::rc::Rc::new(move || {
        if let Some(group) = weak_group.upgrade() {
            group.set_visible(
                weak_items
                    .iter()
                    // is_visible() includes ancestors: once this group hides,
                    // that would prevent its children from ever revealing it.
                    .any(|item| item.upgrade().is_some_and(|item| item.get_visible())),
            );
        }
    });
    for item in items {
        let update = update.clone();
        item.connect_visible_notify(move |_| update());
    }
    update();
}

fn block_owner(glyph: NerdIcon, tooltip: &str) -> gtk::Box {
    let owner = gtk::Box::new(gtk::Orientation::Vertical, 0);
    owner.set_css_classes(&bar_item::classes(&[bar_item::SQUARE_CLASS]));
    owner.set_tooltip_text(Some(tooltip));
    owner.update_property(&[gtk::accessible::Property::Label(tooltip)]);
    let icon = gtk::Label::new(None);
    icon.set_css_classes(&["nerdicon", bar_item::ICON_CLASS]);
    icon.set_hexpand(true);
    icon.set_nerd_icon(glyph);
    owner.append(&icon);
    owner
}
