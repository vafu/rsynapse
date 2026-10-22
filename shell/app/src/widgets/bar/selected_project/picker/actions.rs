use shell_core::gtk::{self, prelude::*};
use std::{cell::Cell, rc::Rc};

mod scoped;
pub(super) use scoped::ScopedAction;

#[zbus::proxy(
    interface = "org.rsynapse.Proj.Manager1",
    default_service = "org.rsynapse.Proj",
    default_path = "/org/rsynapse/Proj"
)]
trait Projects {
    fn remove_project(&self, id: &str) -> zbus::Result<bool>;
    fn rename_project(
        &self,
        id: &str,
        name: &str,
    ) -> zbus::Result<shell_core::source::proj::ProjectInfo>;
    fn add_project_labels(&self, project_id: &str, labels: &[String]) -> zbus::Result<Vec<String>>;
    fn remove_project_labels(
        &self,
        project_id: &str,
        labels: &[String],
    ) -> zbus::Result<Vec<String>>;
}
#[derive(Clone)]
pub(super) enum Action {
    Assign { workspace: String, path: String },
    Unassign(String),
    Remove { id: String, name: String },
    Rename { id: String, name: String },
    AddLabel { id: String, label: String },
    RemoveLabel { id: String, label: String },
}
pub(super) struct Actions {
    busy: Cell<bool>,
    overlay: gtk::glib::WeakRef<adw::ToastOverlay>,
}
impl Actions {
    pub fn new(overlay: &adw::ToastOverlay) -> Rc<Self> {
        Rc::new(Self {
            busy: Cell::new(false),
            overlay: overlay.downgrade(),
        })
    }
    pub fn run(self: &Rc<Self>, action: Action) {
        self.run_with_completion(action, |_| {});
    }
    pub fn run_with_completion(
        self: &Rc<Self>,
        action: Action,
        complete: impl FnOnce(bool) + 'static,
    ) {
        if let Some(task) = self.run_scoped(action, move |success| {
            complete(success);
            true
        }) {
            task.detach();
        }
    }
}

impl Action {
    async fn execute(self) -> zbus::Result<()> {
        match self {
            Self::Assign { workspace, path } => {
                super::super::init::bind_workspace(workspace, &path, false).await?;
            }
            Self::Unassign(workspace) => super::super::init::unassign_workspace(workspace).await?,
            other => {
                let conn = zbus::Connection::session().await?;
                let proxy = ProjectsProxy::new(&conn).await?;
                match other {
                    Self::Remove { id, .. } => {
                        proxy.remove_project(&id).await?;
                    }
                    Self::Rename { id, name } => {
                        proxy.rename_project(&id, &name).await?;
                    }
                    Self::AddLabel { id, label } => {
                        proxy.add_project_labels(&id, &[label]).await?;
                    }
                    Self::RemoveLabel { id, label } => {
                        proxy.remove_project_labels(&id, &[label]).await?;
                    }
                    _ => unreachable!(),
                }
            }
        }
        Ok(())
    }

    fn success_text(&self) -> String {
        match self {
            Self::Assign { .. } => "Project assigned".to_owned(),
            Self::Unassign(_) => "Project unassigned".to_owned(),
            Self::Remove { name, .. } => format!("Removed project “{name}”"),
            Self::Rename { name, .. } => format!("Renamed project to “{name}”"),
            Self::AddLabel { label, .. } => format!("Added label “{label}”"),
            Self::RemoveLabel { label, .. } => format!("Removed label “{label}”"),
        }
    }
}

pub(super) fn icon_button(icon: &str, label: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.add_css_class("flat");
    button.set_valign(gtk::Align::Center);
    button.set_tooltip_text(Some(label));
    button.update_property(&[gtk::accessible::Property::Label(label)]);
    button
}
