use shell_core::gtk::{self, prelude::*};
use std::{cell::Cell, rc::Rc};

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
}
#[derive(Clone)]
pub(super) enum Action {
    Assign { workspace: u64, path: String },
    Unassign(u64),
    Remove { id: String, name: String },
    Rename { id: String, name: String },
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
        if self.busy.replace(true) {
            complete(false);
            return;
        }
        // Keep the chooser anchor sensitive: disabling it closes its popover.
        // The in-flight guard above serializes actions without collapsing it.
        let (sender, receiver) = async_channel::bounded(1);
        let command = action.clone();
        relm4::spawn(async move {
            let result = async {
                match command {
                    Action::Assign { workspace, path } => {
                        super::super::init::bind_workspace(workspace, &path, false).await?;
                    }
                    Action::Unassign(workspace) => {
                        super::super::init::unassign_workspace(workspace).await?
                    }
                    other => {
                        let conn = zbus::Connection::session().await?;
                        let proxy = ProjectsProxy::new(&conn).await?;
                        match other {
                            Action::Remove { id, .. } => {
                                proxy.remove_project(&id).await?;
                            }
                            Action::Rename { id, name } => {
                                proxy.rename_project(&id, &name).await?;
                            }
                            _ => unreachable!(),
                        }
                    }
                }
                Ok::<_, zbus::Error>(())
            }
            .await;
            let _ = sender.send(result.map_err(|e| e.to_string())).await;
        });
        let current = self.clone();
        gtk::glib::MainContext::default().spawn_local(async move {
            let result = receiver.recv().await;
            current.busy.set(false);
            complete(matches!(&result, Ok(Ok(()))));
            let Some(overlay) = current.overlay.upgrade() else {
                return;
            };
            let toast = match result {
                Ok(Ok(())) => {
                    let text = match &action {
                        Action::Assign { .. } => "Project assigned".to_owned(),
                        Action::Unassign(_) => "Project unassigned".to_owned(),
                        Action::Remove { name, .. } => format!("Removed project “{name}”"),
                        Action::Rename { name, .. } => format!("Renamed project to “{name}”"),
                    };
                    adw::Toast::new(&text)
                }
                Ok(Err(error)) => adw::Toast::new(&error),
                Err(_) => return,
            };
            toast.set_use_markup(false);
            overlay.add_toast(toast);
        });
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
