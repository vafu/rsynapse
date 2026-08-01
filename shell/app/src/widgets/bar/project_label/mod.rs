mod input;
mod source;
mod view;

#[cfg(test)]
mod test;

use relm4::prelude::*;
use shell_core::gtk::{self, prelude::*};

use self::{
    input::ProjectLabelInput,
    source::{
        ProjectLabelVm, WorkspaceIconChoice, clear_project_icon_override, project_label_vm,
        set_project_icon_override,
    },
    view::*,
};

use super::{
    WorkspaceNode,
    bar_indicator::{self, BarIndicatorExt},
};
use crate::{
    hints::hints_active,
    widgets::nerd_icon::{NerdIcon, NerdIconLabelExt},
};

#[derive(Debug)]
#[shell_macros::model(module = project_label_sources)]
pub(super) struct ProjectLabel {
    pub workspace: WorkspaceNode,

    #[source(project_label_vm(workspace.workspace.clone()))]
    pub vm: ProjectLabelVm,

    #[source(workspace.workspace.active())]
    pub selected: bool,

    #[source(hints_active())]
    pub hints_active: bool,
}

#[shell_macros::component(
    module = project_label_sources,
    model = ProjectLabel
)]
#[relm4::component(pub(crate))]
impl SimpleComponent for ProjectLabel {
    type Init = WorkspaceNode;
    type Input = ProjectLabelInput;
    type Output = ();

    view! {
        gtk::Revealer {
            #[watch]
            set_reveal_child: workspace_visible(&model.vm, model.selected),
            set_transition_type: gtk::RevealerTransitionType::FadeSlideDown,
            set_transition_duration: 150,
            set_halign: gtk::Align::Center,
            set_valign: gtk::Align::Center,

            gtk::MenuButton {
                set_css_classes: &["flat", "workspace-icon-menu-button"],
                set_always_show_arrow: false,
                set_has_frame: false,
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Center,

                #[watch]
                set_tooltip_text: Some(project_tooltip(&model.vm, &model.workspace).as_str()),

                #[wrap(Some)]
                set_popover = &gtk::Popover {
                    add_css_class: "menu",

                    gtk::Box {
                        add_css_class: "workspace-icon-picker",
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 2,

                        #[name = "icon_auto_button"]
                        gtk::Button {
                            set_css_classes: &auto_icon_button_classes(&model.vm),
                            #[watch]
                            set_visible: auto_icon_visible(&model.vm),

                            gtk::Label {
                                set_css_classes: &["bar-indicator-icon", "nerdicon"],
                                set_halign: gtk::Align::Center,
                                set_valign: gtk::Align::Center,
                                set_nerd_icon: NerdIcon::automatic(),
                            }
                        },

                        #[name = "icon_candidate_0"]
                        gtk::Button {
                            #[watch]
                            set_visible: icon_candidate_visible(&model.vm, 0),
                            #[watch]
                            set_css_classes: &icon_candidate_button_classes(&model.vm, 0),

                            gtk::Label {
                                set_css_classes: &["bar-indicator-icon", "nerdicon"],
                                set_halign: gtk::Align::Center,
                                set_valign: gtk::Align::Center,
                                #[watch]
                                set_nerd_icon: icon_candidate_render(&model.vm, 0),
                            }
                        },

                        #[name = "icon_candidate_1"]
                        gtk::Button {
                            #[watch]
                            set_visible: icon_candidate_visible(&model.vm, 1),
                            #[watch]
                            set_css_classes: &icon_candidate_button_classes(&model.vm, 1),

                            gtk::Label {
                                set_css_classes: &["bar-indicator-icon", "nerdicon"],
                                set_halign: gtk::Align::Center,
                                set_valign: gtk::Align::Center,
                                #[watch]
                                set_nerd_icon: icon_candidate_render(&model.vm, 1),
                            }
                        },

                        #[name = "icon_candidate_2"]
                        gtk::Button {
                            #[watch]
                            set_visible: icon_candidate_visible(&model.vm, 2),
                            #[watch]
                            set_css_classes: &icon_candidate_button_classes(&model.vm, 2),

                            gtk::Label {
                                set_css_classes: &["bar-indicator-icon", "nerdicon"],
                                set_halign: gtk::Align::Center,
                                set_valign: gtk::Align::Center,
                                #[watch]
                                set_nerd_icon: icon_candidate_render(&model.vm, 2),
                            }
                        },

                        #[name = "icon_candidate_3"]
                        gtk::Button {
                            #[watch]
                            set_visible: icon_candidate_visible(&model.vm, 3),
                            #[watch]
                            set_css_classes: &icon_candidate_button_classes(&model.vm, 3),

                            gtk::Label {
                                set_css_classes: &["bar-indicator-icon", "nerdicon"],
                                set_halign: gtk::Align::Center,
                                set_valign: gtk::Align::Center,
                                #[watch]
                                set_nerd_icon: icon_candidate_render(&model.vm, 3),
                            }
                        },

                        #[name = "icon_candidate_4"]
                        gtk::Button {
                            #[watch]
                            set_visible: icon_candidate_visible(&model.vm, 4),
                            #[watch]
                            set_css_classes: &icon_candidate_button_classes(&model.vm, 4),

                            gtk::Label {
                                set_css_classes: &["bar-indicator-icon", "nerdicon"],
                                set_halign: gtk::Align::Center,
                                set_valign: gtk::Align::Center,
                                #[watch]
                                set_nerd_icon: icon_candidate_render(&model.vm, 4),
                            }
                        }
                    }
                },

                #[wrap(Some)]
                set_child = &gtk::Box {
                    set_bar_indicator_size: bar_indicator::SIZE,
                    #[watch]
                    set_css_classes: &project_group_classes(&model.vm, model.selected),

                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    set_hexpand: false,
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 1,

                    #[name = "project_icon_label"]
                    gtk::Label {
                        set_widget_name: "workspace-project-icon",
                        set_css_classes: &["bar-indicator-icon", "nerdicon"],
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,
                        set_hexpand: true,
                        set_xalign: 0.5,

                        #[watch]
                        set_nerd_icon: project_icon_render(&model.vm),
                    },

                    #[name = "title_revealer"]
                    gtk::Revealer {
                        set_reveal_child: false,
                        set_halign: gtk::Align::Start,
                        set_hexpand: false,
                        set_transition_type: gtk::RevealerTransitionType::SlideRight,

                        gtk::Box {
                            add_css_class: "bar-indicator-title",
                            set_halign: gtk::Align::Start,
                            set_hexpand: false,
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 4,

                            gtk::Label {
                                set_ellipsize: gtk::pango::EllipsizeMode::End,

                                #[watch]
                                set_label: project_primary(&model.vm, &model.workspace).as_str(),

                                set_max_width_chars: 18,
                                set_xalign: 0.0,
                            },
                        }
                    },

                    gtk::Label {
                        add_css_class: "bar-badge",
                        add_css_class: "workspace-number-badge",

                        #[watch]
                        set_label: workspace_badge_label(model.vm.index).as_str(),

                        #[watch]
                        set_visible: model.hints_active,

                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,
                    },
                }
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = ProjectLabel::new(init);
        let widgets = view_output!();

        let input_sender = sender.input_sender().clone();
        let auto_button = widgets.icon_auto_button.clone();
        let auto_button_for_signal = auto_button.clone();
        auto_button.connect_clicked(move |_| {
            close_button_popover(&auto_button_for_signal);
            input_sender.emit(ProjectLabelInput::ClearIconOverride);
        });
        connect_icon_candidate_button(&widgets.icon_candidate_0, sender.input_sender().clone(), 0);
        connect_icon_candidate_button(&widgets.icon_candidate_1, sender.input_sender().clone(), 1);
        connect_icon_candidate_button(&widgets.icon_candidate_2, sender.input_sender().clone(), 2);
        connect_icon_candidate_button(&widgets.icon_candidate_3, sender.input_sender().clone(), 3);
        connect_icon_candidate_button(&widgets.icon_candidate_4, sender.input_sender().clone(), 4);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            ProjectLabelInput::Source(msg) => ProjectLabel::update(self, msg),
            ProjectLabelInput::SetIconOverride(index) => {
                let Some(candidate) = self.vm.project_icon_candidates.get(index) else {
                    return;
                };
                set_project_icon_override(
                    self.vm.workspace_id,
                    WorkspaceIconChoice::from(candidate),
                    self.vm.project_icon_input.clone(),
                );
            }
            ProjectLabelInput::ClearIconOverride => {
                clear_project_icon_override(self.vm.workspace_id)
            }
        }
    }
}
