use data_sources as source;
use data_sources::StateSignal;
use relm4::prelude::*;
use shell_core::gtk::{self, prelude::*};

use super::{
    GitPart, ProjectInitializer, SelectedWorkspaceView, branch_for_clipboard, branch_icon,
    branch_label, branch_visible, first_separator_visible, git_loading, git_part_icon,
    git_part_visible, icon_name, selected_project_status, title_label, tooltip, visible,
};
use crate::widgets::nerd_icon::NerdIconLabelExt;

#[derive(Debug)]
pub(crate) enum SelectedProjectItemInput {
    Source(Box<selected_project_item_sources::Msg>),
    OpenWorkspaceSelector,
    CopyBranch,
    OpenProjectPicker,
}

impl From<selected_project_item_sources::Msg> for SelectedProjectItemInput {
    fn from(message: selected_project_item_sources::Msg) -> Self {
        Self::Source(Box::new(message))
    }
}

#[derive(Debug)]
pub(crate) enum SelectedProjectItemOutput {
    OpenWorkspaceSelector,
    CopyBranch(String),
}

#[derive(Debug)]
#[shell_macros::model(module = selected_project_item_sources)]
pub(crate) struct SelectedProjectItem {
    output_scope: StateSignal<Option<String>>,
    initializer: Option<ProjectInitializer>,

    #[source(source::switch_map(output_scope.observable(), selected_project_status))]
    view: SelectedWorkspaceView,
}

#[shell_macros::component(
    module = selected_project_item_sources,
    model = SelectedProjectItem
)]
#[relm4::component(pub(crate))]
impl SimpleComponent for SelectedProjectItem {
    type Init = StateSignal<Option<String>>;
    type Input = SelectedProjectItemInput;
    type Output = SelectedProjectItemOutput;

    view! {
        #[root]
        gtk::Box {
            set_widget_name: "project-git-group",
            update_property: &[gtk::accessible::Property::Label("Project and Git")],
            #[watch]
            set_css_classes: &super::classes(&model.view),
            #[watch]
            set_visible: visible(&model.view),
            #[watch]
            set_tooltip_text: Some(tooltip(&model.view).as_str()),
            set_halign: gtk::Align::Center,
            set_valign: gtk::Align::Center,
            set_orientation: gtk::Orientation::Horizontal,

            #[name = "workspace_title_button"]
            gtk::Button {
                add_css_class: "flat",
                add_css_class: "selected-project-branch-button",
                set_has_frame: false,
                set_tooltip_text: Some("Workspace selector"),
                connect_clicked => SelectedProjectItemInput::OpenWorkspaceSelector,
                #[wrap(Some)]
                set_child = &adw::ButtonContent {
                    add_css_class: "selected-project-segment",
                    add_css_class: "selected-project-main",
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    set_can_shrink: true,
                    #[watch]
                    set_icon_name: icon_name(&model.view),
                    #[watch]
                    set_label: title_label(&model.view),
                }
            },

            gtk::Label {
                add_css_class: "selected-project-separator",
                set_label: "·",
                #[watch]
                set_visible: first_separator_visible(&model.view),
            },

            #[name = "branch_copy_button"]
            gtk::Button {
                add_css_class: "flat",
                add_css_class: "selected-project-branch-button",
                set_has_frame: false,
                #[watch]
                set_visible: branch_visible(&model.view),
                connect_clicked => SelectedProjectItemInput::CopyBranch,

                #[wrap(Some)]
                set_child = &gtk::Box {
                    add_css_class: "selected-project-segment",
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    set_orientation: gtk::Orientation::Horizontal,

                    gtk::Label {
                        set_css_classes: &[
                            "selected-project-meta-icon",
                            "selected-project-branch-icon",
                            "nerdicon",
                        ],
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,
                        set_nerd_icon: branch_icon(),
                    },

                    #[name = "branch_copy_label"]
                    gtk::Label {
                        add_css_class: "selected-project-branch-label",
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_valign: gtk::Align::Center,
                        set_xalign: 0.0,
                        #[watch]
                        set_label: branch_label(&model.view),
                    },

                    gtk::Label {
                        add_css_class: "selected-project-git-icon",
                        add_css_class: "nerdicon",
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_visible: git_part_visible(&model.view, GitPart::Dirty),
                        set_label: git_part_icon(GitPart::Dirty),
                    },

                    gtk::Label {
                        add_css_class: "selected-project-git-icon",
                        add_css_class: "nerdicon",
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_visible: git_part_visible(&model.view, GitPart::Untracked),
                        set_label: git_part_icon(GitPart::Untracked),
                    },

                    gtk::Label {
                        add_css_class: "selected-project-git-icon",
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_visible: git_part_visible(&model.view, GitPart::Ahead),
                        set_label: git_part_icon(GitPart::Ahead),
                    },

                    gtk::Label {
                        add_css_class: "selected-project-git-icon",
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_visible: git_part_visible(&model.view, GitPart::Behind),
                        set_label: git_part_icon(GitPart::Behind),
                    },

                    gtk::Label {
                        add_css_class: "selected-project-git-icon",
                        add_css_class: "nerdicon",
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_visible: git_part_visible(&model.view, GitPart::Merging),
                        set_label: git_part_icon(GitPart::Merging),
                    },

                    gtk::Label {
                        add_css_class: "selected-project-git-icon",
                        add_css_class: "nerdicon",
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_visible: git_part_visible(&model.view, GitPart::Rebasing),
                        set_label: git_part_icon(GitPart::Rebasing),
                    },

                    gtk::Spinner {
                        add_css_class: "selected-project-git-spinner",
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_visible: git_loading(&model.view),
                        #[watch]
                        set_spinning: git_loading(&model.view),
                    },
                }
            },
        }
    }

    fn init(
        output_scope: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut model = SelectedProjectItem::new(output_scope, None);
        let widgets = view_output!();
        model.initializer = Some(ProjectInitializer::new(&widgets.workspace_title_button));
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            SelectedProjectItemInput::Source(message) => {
                SelectedProjectItem::update(self, *message);
            }
            SelectedProjectItemInput::OpenWorkspaceSelector => {
                let _ = sender.output(SelectedProjectItemOutput::OpenWorkspaceSelector);
            }
            SelectedProjectItemInput::CopyBranch => {
                let branch = self
                    .view
                    .project
                    .as_ref()
                    .and_then(|project| project.branch_full.as_deref());
                if let Some(branch) = branch_for_clipboard(branch) {
                    let _ = sender.output(SelectedProjectItemOutput::CopyBranch(branch.to_owned()));
                }
            }
            SelectedProjectItemInput::OpenProjectPicker => {
                if let Some(initializer) = &self.initializer {
                    initializer.open_focused();
                }
            }
        }
    }
}
