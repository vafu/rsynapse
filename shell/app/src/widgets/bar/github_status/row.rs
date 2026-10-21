use relm4::prelude::*;
use shell_core::gtk::{self, prelude::*};

use super::super::bar_item;
use super::{actions, source::PrEntry};

#[derive(Debug)]
pub(super) enum Input {
    Open,
    MarkSeen,
}

pub(super) struct PullRequestRow {
    entry: PrEntry,
}

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
            set_tooltip_text: Some(format!("{}\n{}\nLeft-click to open · Right-click to mark seen", model.entry.title, model.entry.url).as_str()),
            connect_clicked => Input::Open,

            #[wrap(Some)]
            set_child = &gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 4,
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    gtk::Label {
                        set_label: &model.entry.title,
                        set_xalign: 0.0,
                        set_wrap: true,
                        set_wrap_mode: gtk::pango::WrapMode::WordChar,
                        set_natural_wrap_mode: gtk::NaturalWrapMode::None,
                        set_hexpand: true,
                    },
                    #[local_ref]
                    unseen_badge -> gtk::Box {
                        set_visible: model.entry.unseen,
                        set_valign: gtk::Align::Center,
                    },
                },
                gtk::Label {
                    set_label: &model.entry.detail,
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
        let model = PullRequestRow { entry };
        let unseen_badge = bar_item::unseen_badge();
        let widgets = view_output!();
        let secondary_click = gtk::GestureClick::new();
        secondary_click.set_button(gtk::gdk::BUTTON_SECONDARY);
        secondary_click.set_propagation_phase(gtk::PropagationPhase::Capture);
        secondary_click.connect_pressed(|gesture, _, _, _| {
            let _ = gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        secondary_click.connect_released(move |_, _, _, _| {
            sender.input(Input::MarkSeen);
        });
        root.add_controller(secondary_click);
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Self::Input, _sender: ComponentSender<Self>) {
        let entry = self.entry.clone();
        relm4::spawn(async move {
            let result = match input {
                Input::Open => actions::open_pr(entry).await,
                Input::MarkSeen => actions::mark_seen(entry).await,
            };
            if let Err(error) = result {
                eprintln!("[github-status] {error}");
            }
        });
    }
}
