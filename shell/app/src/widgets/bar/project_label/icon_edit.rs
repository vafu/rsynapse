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
    shell_core::source::write::spawn("project icon", async move {
        let conn = shell_core::source::write::session_connection().await?;
        let proxy = shell_core::source::proj::ManagerProxy::new(conn).await?;
        let _: shell_core::source::proj::ProjectInfo = match glyph {
            Some(glyph) => proxy.set_project_icon(&id, &glyph).await?,
            None => proxy.clear_project_icon(&id).await?,
        };
        Ok::<_, zbus::Error>(())
    });
}
