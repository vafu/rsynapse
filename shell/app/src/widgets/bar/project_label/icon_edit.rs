use super::source::{ProjectLabelVm, WorkspaceIconChoice};
#[derive(Clone, Debug)]
pub(super) enum IconTarget {
    Project(String),
    Workspace { id: u64, name: String },
}
impl IconTarget {
    pub fn from_vm(vm: &ProjectLabelVm) -> Option<Self> {
        if let Some(id) = &vm.project_id {
            Some(Self::Project(id.clone()))
        } else if vm.has_project {
            None
        } else {
            vm.workspace_id.map(|id| Self::Workspace {
                id,
                name: vm.workspace_name.clone(),
            })
        }
    }
    pub fn set(&self, icon: WorkspaceIconChoice, input: String) {
        match self {
            Self::Workspace { id, name } => {
                super::source::set_project_icon_override(Some(*id), name.clone(), icon, input)
            }
            Self::Project(id) => write(id.clone(), Some(icon.glyph)),
        }
    }
    pub fn clear(&self) {
        match self {
            Self::Workspace { id, name } => {
                super::source::clear_project_icon_override(Some(*id), name.clone())
            }
            Self::Project(id) => write(id.clone(), None),
        }
    }
}
fn write(id: String, glyph: Option<String>) {
    relm4::spawn(async move {
        let result = async {
            let conn = zbus::Connection::session().await?;
            let proxy = zbus::Proxy::new(
                &conn,
                "org.rsynapse.Proj",
                "/org/rsynapse/Proj",
                "org.rsynapse.Proj.Manager1",
            )
            .await?;
            let _: shell_core::source::proj::ProjectInfo = match glyph {
                Some(g) => proxy.call("SetProjectIcon", &(id, g)).await?,
                None => proxy.call("ClearProjectIcon", &(id,)).await?,
            };
            Ok::<_, zbus::Error>(())
        }
        .await;
        if let Err(error) = result {
            eprintln!("[project-icon] {error}");
        }
    });
}
