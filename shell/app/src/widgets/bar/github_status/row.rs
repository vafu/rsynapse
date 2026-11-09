use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::*},
    source::{github, rx::Observable as _},
};

use super::super::bar_item;
use super::{
    actions,
    category::Category,
    source::{self, PrEntry, PrRowView},
};

#[derive(Debug)]
pub(super) enum Input {
    Source(Box<pull_request_row_sources::Msg>),
    Open,
    MarkSeen,
}

impl From<pull_request_row_sources::Msg> for Input {
    fn from(message: pull_request_row_sources::Msg) -> Self {
        Self::Source(Box::new(message))
    }
}

#[shell_macros::model(module = pull_request_row_sources)]
pub(super) struct PullRequestRow {
    entry: PrEntry,

    #[source({
        let category = entry.category;
        github::activity_entry(entry.url.clone())
            .map(move |activity| source::row_view(category, activity))
            .box_it()
    })]
    view: PrRowView,
}

#[shell_macros::component(module = pull_request_row_sources, model = PullRequestRow)]
#[relm4::component(pub(crate))]
impl SimpleComponent for PullRequestRow {
    type Init = PrEntry;
    type Input = Input;
    type Output = ();

    view! {
        #[root]
        gtk::Button {
            set_css_classes: &["flat", "github-pr-row"],
            set_hexpand: true,
            #[watch]
            set_tooltip_text: Some(model.view.tooltip(&model.entry.url).as_str()),
            connect_clicked => Input::Open,

            #[wrap(Some)]
            set_child = &gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 4,
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    gtk::Label {
                        #[watch]
                        set_label: &model.view.title,
                        set_xalign: 0.0,
                        set_wrap: true,
                        set_wrap_mode: gtk::pango::WrapMode::WordChar,
                        set_natural_wrap_mode: gtk::NaturalWrapMode::None,
                        set_hexpand: true,
                    },
                    #[local_ref]
                    unseen_badge -> gtk::Box {
                        #[watch]
                        set_visible: model.view.unseen,
                        set_valign: gtk::Align::Center,
                    },
                },
                gtk::Label {
                    #[watch]
                    set_label: &model.view.detail,
                    set_xalign: 0.0,
                    set_wrap: true,
                    set_natural_wrap_mode: gtk::NaturalWrapMode::None,
                    add_css_class: "dim-label",
                },
            },
        }
    }

    fn init(
        entry: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = PullRequestRow::new(entry);
        let unseen_badge = bar_item::unseen_badge();
        let widgets = view_output!();
        let secondary_click = gtk::GestureClick::new();
        secondary_click.set_button(gtk::gdk::BUTTON_SECONDARY);
        secondary_click.set_propagation_phase(gtk::PropagationPhase::Capture);
        secondary_click.connect_pressed(|gesture, _, _, _| {
            let _ = gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        let sender_for_seen = sender.clone();
        secondary_click.connect_released(move |_, _, _, _| {
            sender_for_seen.input(Input::MarkSeen);
        });
        root.add_controller(secondary_click);
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Self::Input, _sender: ComponentSender<Self>) {
        match input {
            Input::Source(message) => PullRequestRow::update(self, *message),
            Input::Open => {
                let url = self.entry.url.clone();
                let revision = self.view.revision.clone();
                relm4::spawn(async move {
                    if let Err(error) = actions::open_pr(url, revision).await {
                        eprintln!("[github-status] {error}");
                    }
                });
            }
            Input::MarkSeen => {
                let url = self.entry.url.clone();
                let revision = self.view.revision.clone();
                relm4::spawn(async move {
                    if let Err(error) = actions::mark_seen(url, revision).await {
                        eprintln!("[github-status] {error}");
                    }
                });
            }
        }
    }
}
