mod input;
mod source;
mod view;

#[cfg(test)]
mod test;

use nerd_icon_picker::NerdIconPicker;
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
use crate::{hints::hints_active, widgets::nerd_icon::NerdIconLabelExt};

#[derive(Debug)]
#[shell_macros::model(module = project_label_sources)]
pub(super) struct ProjectLabel {
    pub workspace: WorkspaceNode,
    icon_picker: NerdIconPicker,

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

            gtk::Overlay {
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
                    #[name = "icon_popover"]
                    set_popover = &gtk::Popover {
                        add_css_class: "menu",

                        #[local_ref]
                        icon_picker_root -> gtk::Box {},
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
                },

                add_overlay = &gtk::Box {
                    add_css_class: "bar-badge",
                    add_css_class: "agent-unseen-badge",
                    add_css_class: "workspace-agent-unseen-badge",
                    set_can_target: false,
                    set_width_request: 8,
                    set_height_request: 8,

                    #[watch]
                    set_visible: workspace_agent_unseen_visible(&model.vm),

                    set_halign: gtk::Align::End,
                    set_valign: gtk::Align::Start,
                }
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let icon_picker = NerdIconPicker::new();
        let icon_picker_root = icon_picker.widget().clone();
        let model = ProjectLabel::new(init, icon_picker);
        let widgets = view_output!();

        let input_sender = sender.input_sender().clone();
        let icon_popover = widgets.icon_popover.clone();
        model.icon_picker.connect_icon_selected(move |icon| {
            icon_popover.popdown();
            input_sender.emit(ProjectLabelInput::SetIconOverride(icon.glyph().to_owned()));
        });
        let input_sender = sender.input_sender().clone();
        let icon_popover = widgets.icon_popover.clone();
        model.icon_picker.connect_reset(move || {
            icon_popover.popdown();
            input_sender.emit(ProjectLabelInput::ClearIconOverride);
        });
        sync_icon_picker(&model);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            ProjectLabelInput::Source(msg) => {
                ProjectLabel::update(self, msg);
                sync_icon_picker(self);
            }
            ProjectLabelInput::SetIconOverride(glyph) => {
                let Some(icon) = WorkspaceIconChoice::new(glyph) else {
                    return;
                };
                set_project_icon_override(
                    self.vm.workspace_id,
                    icon,
                    self.vm.project_icon_input.clone(),
                );
            }
            ProjectLabelInput::ClearIconOverride => {
                clear_project_icon_override(self.vm.workspace_id)
            }
        }
    }
}

fn sync_icon_picker(model: &ProjectLabel) {
    let icons = model
        .vm
        .project_icon_candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            nerd_icon_picker::NerdIcon::specific(
                format!("Suggested icon {}", index + 1),
                candidate.glyph.clone(),
            )
        })
        .collect();
    model.icon_picker.set_specific_icons(icons);
    model
        .icon_picker
        .set_selected_glyph(Some(&model.vm.project_icon_glyph));
    model
        .icon_picker
        .set_reset_visible(model.vm.project_icon_overridden);
}
