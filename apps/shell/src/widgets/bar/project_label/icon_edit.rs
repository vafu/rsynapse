use super::source::{ProjectLabelVm, WorkspaceIconChoice};
#[derive(Clone, Debug)]
pub(super) enum IconTarget {
    Project(String),
    Workspace { id: String, name: String },
}
impl IconTarget {
    pub fn from_vm(vm: &ProjectLabelVm) -> Option<Self> {
        if let Some(id) = &vm.project_id {
            Some(Self::Project(id.clone()))
        } else if vm.has_project {
            None
        } else {
            vm.workspace_id.clone().map(|id| Self::Workspace {
                id,
                name: vm.workspace_name.clone(),
            })
        }
    }
    pub fn set(&self, icon: WorkspaceIconChoice, input: String) {
        match self {
            Self::Workspace { id, name } => super::source::set_project_icon_override(
                Some(id.clone()),
                name.clone(),
                icon,
                input,
            ),
            Self::Project(id) => write(id.clone(), Some(icon.glyph)),
        }
    }
    pub fn clear(&self) {
        match self {
            Self::Workspace { id, name } => {
                super::source::clear_project_icon_override(Some(id.clone()), name.clone())
            }
            Self::Project(id) => write(id.clone(), None),
        }
    }
}
fn write(id: String, glyph: Option<String>) {
    data_sources::proj::set_project_icon(id, glyph);
}
