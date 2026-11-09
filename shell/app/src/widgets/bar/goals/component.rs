use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::*},
    source::StateSignal,
};

use super::GoalBarVm;

#[derive(Debug)]
pub(crate) enum GoalIndicatorInput {
    Source(Box<goal_indicator_sources::Msg>),
    OpenRemarked,
}

impl From<goal_indicator_sources::Msg> for GoalIndicatorInput {
    fn from(message: goal_indicator_sources::Msg) -> Self {
        Self::Source(Box::new(message))
    }
}

#[derive(Debug)]
pub(crate) enum GoalIndicatorOutput {
    OpenRemarked,
}

#[derive(Debug)]
#[shell_macros::model(module = goal_indicator_sources)]
pub(crate) struct GoalIndicator {
    output_scope: StateSignal<Option<String>>,

    #[source(shell_core::source::switch_map(output_scope.observable(), super::goal_bar))]
    goals: GoalBarVm,
}

#[shell_macros::component(module = goal_indicator_sources, model = GoalIndicator)]
#[relm4::component(pub(crate))]
impl SimpleComponent for GoalIndicator {
    type Init = StateSignal<Option<String>>;
    type Input = GoalIndicatorInput;
    type Output = GoalIndicatorOutput;

    view! {
        #[root]
        gtk::Overlay {
            set_widget_name: "goal-left-section",
            set_hexpand: true,
            set_halign: gtk::Align::Fill,

            #[wrap(Some)]
            set_child = &gtk::Box {
                set_hexpand: true,
            },

            #[name = "goal_button"]
            add_overlay = &gtk::Button {
                add_css_class: "flat",
                add_css_class: "goal-tracking-button",
                set_halign: gtk::Align::End,
                set_valign: gtk::Align::Center,
                set_has_frame: false,
                connect_clicked => GoalIndicatorInput::OpenRemarked,
                #[watch]
                set_visible: model.goals.project_id.is_some(),
                #[watch]
                set_tooltip_text: Some(model.goals.tooltip.as_str()),
                #[wrap(Some)]
                set_child = &adw::ButtonContent {
                    set_halign: gtk::Align::End,
                    set_valign: gtk::Align::Center,
                    set_icon_name: "view-list-symbolic",
                    set_can_shrink: true,
                    #[watch]
                    set_label: model.goals.title.as_str(),
                }
            }
        }
    }

    fn init(
        output_scope: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = GoalIndicator::new(output_scope);
        let widgets = view_output!();
        root.set_measure_overlay(&widgets.goal_button, false);
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            GoalIndicatorInput::Source(message) => GoalIndicator::update(self, *message),
            GoalIndicatorInput::OpenRemarked => {
                let _ = sender.output(GoalIndicatorOutput::OpenRemarked);
            }
        }
    }
}
