use relm4::prelude::*;
use shell_core::gtk::{self, gdk, glib, prelude::*};

use super::source::{PendingApproval, pretty_path, respond_to_elicitation};

/// One approval card: header, prompt, detail view, and option buttons.
///
/// Option buttons are built imperatively in `init`: the option list is
/// dynamic per request and the selection highlight is row-local UI state,
/// neither of which can flow through source-driven `#[bind_list]` items.
/// Card content is an immutable snapshot refreshed from the parent list.
pub(super) struct ApprovalCard {
    request: PendingApproval,
    selected: usize,
    option_buttons: Vec<gtk::Button>,
}

#[derive(Debug)]
pub(super) enum ApprovalCardInput {
    Next,
    Previous,
    Answer(usize),
    AnswerSelected,
}

#[relm4::component(pub(super))]
impl SimpleComponent for ApprovalCard {
    type Init = PendingApproval;
    type Input = ApprovalCardInput;
    type Output = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 16,
            set_hexpand: true,
            set_focusable: true,
            add_css_class: "agent-approval-card",

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 12,
                add_css_class: "agent-approval-header",

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_hexpand: true,

                    gtk::Label {
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_max_width_chars: 44,
                        add_css_class: "agent-approval-project",
                        set_label: pretty_path(&init.cwd).as_str(),
                    },

                    gtk::Label {
                        set_xalign: 0.0,
                        add_css_class: "agent-approval-agent",
                        set_label: agent_label(&init).as_str(),
                    },
                },

                gtk::Label {
                    add_css_class: "agent-approval-model",
                    set_label: init.model_name.as_str(),
                },
            },

            gtk::Label {
                set_xalign: 0.0,
                set_wrap: true,
                set_max_width_chars: 92,
                add_css_class: "agent-approval-prompt",
                set_label: init.prompt.as_str(),
            },

            #[name(detail_box)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
            },

            #[name(options_box)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 8,
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();

        if !init.detail_text.is_empty() {
            widgets
                .detail_box
                .append(&detail_view(&init.detail_kind, &init.detail_text));
        }

        let mut model = ApprovalCard {
            request: init,
            selected: 0,
            option_buttons: Vec::new(),
        };
        if !model.request.session_title.is_empty() {
            root.set_tooltip_text(Some(model.request.session_title.as_str()));
        }
        for (index, option) in model.request.options.iter().enumerate() {
            let description = model
                .request
                .option_descriptions
                .get(index)
                .cloned()
                .unwrap_or_default();
            let button = option_button(index, option, &description, &sender);
            widgets.options_box.append(&button);
            model.option_buttons.push(button);
        }
        refresh_selection(&model);

        let key_controller = gtk::EventControllerKey::new();
        let key_sender = sender.input_sender().clone();
        let option_count = model.request.options.len();
        key_controller.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gdk::Key::j || keyval == gdk::Key::Down {
                key_sender.emit(ApprovalCardInput::Next);
                return glib::Propagation::Stop;
            }
            if keyval == gdk::Key::k || keyval == gdk::Key::Up {
                key_sender.emit(ApprovalCardInput::Previous);
                return glib::Propagation::Stop;
            }
            if keyval == gdk::Key::Return
                || keyval == gdk::Key::KP_Enter
                || keyval == gdk::Key::space
            {
                key_sender.emit(ApprovalCardInput::AnswerSelected);
                return glib::Propagation::Stop;
            }
            if let Some(digit) = key_digit(keyval, option_count) {
                key_sender.emit(ApprovalCardInput::Answer(digit));
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        root.add_controller(key_controller);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            ApprovalCardInput::Next => {
                self.selected =
                    (self.selected + 1).min(self.option_buttons.len().saturating_sub(1));
                refresh_selection(self);
            }
            ApprovalCardInput::Previous => {
                self.selected = self.selected.saturating_sub(1);
                refresh_selection(self);
            }
            ApprovalCardInput::Answer(index) => {
                if let Some(option) = self.request.options.get(index).cloned() {
                    self.selected = index;
                    refresh_selection(self);
                    answer(&self.request, &option);
                }
            }
            ApprovalCardInput::AnswerSelected => {
                if let Some(option) = self.request.options.get(self.selected).cloned() {
                    answer(&self.request, &option);
                }
            }
        }
    }
}

fn agent_label(request: &PendingApproval) -> String {
    if request.agent_name.is_empty() {
        "agent".to_owned()
    } else {
        request.agent_name.clone()
    }
}

fn answer(request: &PendingApproval, option: &str) {
    respond_to_elicitation(
        request.session_path.clone(),
        request.request_id.clone(),
        option.to_owned(),
    );
}

fn refresh_selection(card: &ApprovalCard) {
    for (index, button) in card.option_buttons.iter().enumerate() {
        if index == card.selected {
            button.add_css_class("selected");
        } else {
            button.remove_css_class("selected");
        }
    }
}

fn option_button(
    index: usize,
    option: &str,
    description: &str,
    sender: &ComponentSender<ApprovalCard>,
) -> gtk::Button {
    let button = gtk::Button::with_label("");
    button.add_css_class("agent-approval-option");
    button.set_hexpand(true);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let key_label = gtk::Label::new(Some(&(index + 1).to_string()));
    key_label.add_css_class("agent-approval-key");
    row.append(&key_label);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    let label = gtk::Label::new(Some(option));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.set_wrap(true);
    text.append(&label);
    if !description.is_empty() {
        let detail = gtk::Label::new(Some(description));
        detail.set_xalign(0.0);
        detail.set_hexpand(true);
        detail.set_wrap(true);
        detail.add_css_class("agent-approval-option-detail");
        text.append(&detail);
    }
    row.append(&text);
    button.set_child(Some(&row));

    let input = sender.input_sender().clone();
    button.connect_clicked(move |clicked| {
        clicked.add_css_class("selected");
        input.emit(ApprovalCardInput::Answer(index));
    });
    button
}

fn detail_view(detail_kind: &str, detail_text: &str) -> gtk::Widget {
    // No inner ScrolledWindow: the card grows to its natural height and the
    // overlay's outer scroll owns overflow. A nested viewport would clamp the
    // detail to its minimum height instead.
    let structured = matches!(detail_kind, "diff" | "command" | "json");
    let view = gtk::TextView::new();
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_monospace(structured);
    view.set_wrap_mode(if structured {
        gtk::WrapMode::None
    } else {
        gtk::WrapMode::WordChar
    });
    view.set_top_margin(12);
    view.set_bottom_margin(12);
    view.set_left_margin(12);
    view.set_right_margin(12);
    view.add_css_class("agent-approval-detail");
    view.add_css_class("agent-approval-detail-surface");
    view.add_css_class(if structured {
        "agent-approval-detail-monospace"
    } else {
        "agent-approval-detail-text"
    });
    view.buffer().set_text(detail_text);
    view.upcast()
}

fn key_digit(keyval: gdk::Key, option_count: usize) -> Option<usize> {
    const DIGITS: [gdk::Key; 9] = [
        gdk::Key::_1,
        gdk::Key::_2,
        gdk::Key::_3,
        gdk::Key::_4,
        gdk::Key::_5,
        gdk::Key::_6,
        gdk::Key::_7,
        gdk::Key::_8,
        gdk::Key::_9,
    ];
    DIGITS
        .iter()
        .position(|digit| *digit == keyval)
        .filter(|index| *index < option_count)
}
