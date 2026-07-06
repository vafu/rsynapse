mod source;

use relm4::prelude::*;
use shell_core::gtk::{self, prelude::*};

use self::source::{ProjectLabelVm, project_label_vm};

use super::WorkspaceNode;
use crate::{
    hints::hints_active,
    widgets::{BACKGROUND_BLUR_CLASS, material_icon},
};

#[derive(Debug)]
#[shell_macros::model(module = project_label_sources)]
pub(super) struct ProjectLabel {
    pub workspace: WorkspaceNode,

    #[source(project_label_vm(workspace.workspace.clone()))]
    pub vm: ProjectLabelVm,

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
    type Input = project_label_sources::Msg;
    type Output = ();

    view! {
        gtk::Overlay {
            set_halign: gtk::Align::Start,
            set_hexpand: false,

            #[name = "group"]
            gtk::Box {
                #[watch]
                set_css_classes: &project_group_classes(&model.vm),

                set_halign: gtk::Align::Start,
                set_hexpand: false,
                set_orientation: gtk::Orientation::Horizontal,

                #[name = "root_button"]
                gtk::Button {
                    #[watch]
                    set_css_classes: root_button_classes(model.vm.active),

                    #[watch]
                    set_tooltip_text: Some(project_tooltip(&model.vm, &model.workspace).as_str()),

                    set_halign: gtk::Align::Start,
                    set_hexpand: false,

                    gtk::Box {
                        add_css_class: "projects-collapsed-icon",
                        add_css_class: "workspaces-collapsed-icon",
                        set_halign: gtk::Align::Center,
                        set_hexpand: false,

                        #[local_ref]
                        icon -> gtk::Image {
                            #[watch]
                            set_css_classes: project_icon_classes(&model.vm),

                            #[watch]
                            set_icon_name: Some(project_icon_name(&model.vm).as_str()),
                        }
                    }
                },

                #[name = "title_revealer"]
                gtk::Revealer {
                    #[watch]
                    set_reveal_child: model.vm.active,

                    set_halign: gtk::Align::Start,
                    set_hexpand: false,
                    set_transition_type: gtk::RevealerTransitionType::SlideRight,

                    gtk::Box {
                        add_css_class: "button-subgroup",
                        set_halign: gtk::Align::Start,
                        set_hexpand: false,
                        set_orientation: gtk::Orientation::Horizontal,

                        gtk::Box {
                            add_css_class: "projects-title",
                            add_css_class: "workspaces-title",
                            set_halign: gtk::Align::Start,
                            set_hexpand: false,
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 4,

                            gtk::Label {
                                add_css_class: "projects-primary",
                                add_css_class: "workspaces-primary",
                                set_ellipsize: gtk::pango::EllipsizeMode::End,

                                #[watch]
                                set_label: project_primary(&model.vm, &model.workspace).as_str(),

                                set_max_width_chars: 18,
                                set_xalign: 0.0,
                            },

                            gtk::Label {
                                add_css_class: "projects-delimiter",
                                add_css_class: "workspaces-delimiter",

                                #[watch]
                                set_visible: project_secondary(&model.vm).is_some(),

                                set_label: "·",
                                set_xalign: 0.0,
                            },

                            gtk::Label {
                                add_css_class: "projects-secondary",
                                add_css_class: "workspaces-secondary",
                                set_ellipsize: gtk::pango::EllipsizeMode::End,

                                #[watch]
                                set_label: project_secondary(&model.vm).unwrap_or_default().as_str(),

                                #[watch]
                                set_visible: project_secondary(&model.vm).is_some(),

                                set_max_width_chars: 18,
                                set_xalign: 0.0,
                            }
                        }
                    }
                }
            },

            add_overlay = &gtk::Label {
                add_css_class: "barblock-badge",
                add_css_class: "workspace-number-badge",

                #[watch]
                set_label: workspace_badge_label(model.vm.index).as_str(),

                #[watch]
                set_visible: model.hints_active,

                set_halign: gtk::Align::End,
                set_valign: gtk::Align::Start,
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = ProjectLabel::new(init);
        let icon = gtk::Image::new();
        icon.set_css_classes(project_icon_classes(&model.vm));
        icon.set_icon_name(Some(project_icon_name(&model.vm).as_str()));
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }
}

const ROOT_BUTTON_CLASSES: &[&str] = &[
    "projects-root-button",
    "workspaces-root-button",
    "flat",
    "circular",
    "panel-widget",
    "button-subgroup-main",
];
const ROOT_BUTTON_OPEN_CLASSES: &[&str] = &[
    "projects-root-button",
    "workspaces-root-button",
    "flat",
    "circular",
    "panel-widget",
    "button-subgroup-main",
    "opened",
];

fn project_group_classes(vm: &ProjectLabelVm) -> Vec<&'static str> {
    let mut classes = vec![
        "projects-project",
        BACKGROUND_BLUR_CLASS,
        "workspaces-workspace",
        "button-subgroup-expand-right",
    ];

    if vm.active {
        classes.push("current-workspace");
    }
    if vm.urgent || vm.agent.has_attention {
        classes.push("has-attention");
    }
    if vm.agent.has_working {
        classes.push("has-working");
    }
    if vm.empty {
        classes.push("is-empty");
    }

    classes
}

fn root_button_classes(active: bool) -> &'static [&'static str] {
    if active {
        ROOT_BUTTON_OPEN_CLASSES
    } else {
        ROOT_BUTTON_CLASSES
    }
}

fn project_icon(model: &ProjectLabelVm) -> String {
    model
        .project_icon
        .as_deref()
        .and_then(non_empty_text)
        .unwrap_or("view_quilt")
        .to_owned()
}

fn project_icon_name(model: &ProjectLabelVm) -> String {
    let icon = project_icon(model);
    if model.project_icon_is_app {
        icon
    } else {
        material_icon::icon_name(&icon)
    }
}

fn project_icon_classes(model: &ProjectLabelVm) -> &'static [&'static str] {
    if model.project_icon_is_app {
        &["workspace-app-icon"]
    } else {
        &["materialicon"]
    }
}

fn project_primary(model: &ProjectLabelVm, _workspace: &WorkspaceNode) -> String {
    model
        .project_name
        .as_deref()
        .and_then(non_empty_text)
        .map(str::to_owned)
        .unwrap_or_else(|| workspace_title(&model.workspace_name, model.index))
}

fn project_secondary(model: &ProjectLabelVm) -> Option<String> {
    model
        .project_branch
        .as_deref()
        .and_then(non_empty_text)
        .map(str::to_owned)
}

fn project_tooltip(model: &ProjectLabelVm, workspace: &WorkspaceNode) -> String {
    let primary = project_primary(model, workspace);
    match project_secondary(model) {
        Some(secondary) => format!("{primary} · {secondary}"),
        None => primary,
    }
}

fn workspace_title(workspace_name: &str, index: u32) -> String {
    optional_text(Some(workspace_name))
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Workspace {}", index))
}

fn optional_text(value: Option<&str>) -> Option<&str> {
    non_empty_text(value?)
}

fn non_empty_text(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn workspace_badge_label(sort_index: u32) -> String {
    sort_index.to_string()
}
