use shell_core::gtk::{self, prelude::*};
use std::{cell::Cell, rc::Rc};

#[zbus::proxy(
    interface = "org.rsynapse.Steward.Associations1",
    default_service = "org.rsynapse.Steward",
    default_path = "/org/rsynapse/Steward"
)]
trait Associations {
    fn unassign_workspace_project(&self, workspace: u64) -> zbus::Result<()>;
    fn bind_workspace_project(
        &self,
        workspace: u64,
        path: &str,
    ) -> zbus::Result<shell_core::source::proj::ContextInfo>;
    fn get_init_workspace(&self) -> zbus::Result<u64>;
    fn init_workspace_project(
        &self,
        workspace: u64,
        path: &str,
    ) -> zbus::Result<shell_core::source::proj::ContextInfo>;
}

pub(super) async fn unassign_workspace(workspace: u64) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    AssociationsProxy::new(&connection)
        .await?
        .unassign_workspace_project(workspace)
        .await
}

pub(super) async fn bind_workspace(
    workspace: u64,
    path: &str,
    initialize: bool,
) -> zbus::Result<shell_core::source::proj::ContextInfo> {
    let connection = zbus::Connection::session().await?;
    let proxy = AssociationsProxy::new(&connection).await?;
    if initialize {
        proxy.init_workspace_project(workspace, path).await
    } else {
        proxy.bind_workspace_project(workspace, path).await
    }
}

#[derive(Clone)]
pub(in crate::widgets::bar) struct ProjectInitializer {
    button: gtk::glib::WeakRef<gtk::MenuButton>,
    active: Rc<Cell<bool>>,
}

impl ProjectInitializer {
    pub(in crate::widgets::bar) fn new(button: &gtk::MenuButton) -> Self {
        Self {
            button: button.downgrade(),
            active: Rc::new(Cell::new(false)),
        }
    }

    /// Shortcut requests target global focus, even when the primary bar is on
    /// another monitor. Capture its ID before the picker takes focus.
    pub(in crate::widgets::bar) fn open_focused(&self) {
        if self.active.replace(true) {
            return;
        }
        self.sensitive(false);
        let (sender, receiver) = async_channel::bounded(1);
        relm4::spawn(async move {
            let result = async {
                let connection = zbus::Connection::session().await?;
                AssociationsProxy::new(&connection)
                    .await?
                    .get_init_workspace()
                    .await
            }
            .await;
            let _ = sender.send(result.map_err(|e| e.to_string())).await;
        });
        let controller = self.clone();
        gtk::glib::MainContext::default().spawn_local(async move {
            let result = receiver.recv().await;
            controller.active.set(false);
            controller.sensitive(true);
            match result {
                Ok(Ok(id)) => controller.open(id, &format!("workspace {id}"), true),
                Ok(Err(error)) => controller.error(&error),
                Err(_) => {}
            }
        });
    }

    pub(super) fn open_binding(&self, workspace: u64, name: &str, initialize: bool) {
        self.open(workspace, name, initialize);
    }

    fn open(&self, workspace: u64, name: &str, initialize: bool) {
        let Some(_parent) = self.parent() else {
            return;
        };
        if self.active.replace(true) {
            return;
        }
        self.sensitive(false);
        let dialog = gtk::FileDialog::builder()
            .title(format!(
                "{} for {name}",
                if initialize {
                    "Initialize project"
                } else {
                    "Choose project directory"
                }
            ))
            .modal(true)
            .accept_label(if initialize {
                "Initialize project"
            } else {
                "Use project"
            })
            .build();
        let controller = self.clone();
        gtk::glib::MainContext::default().spawn_local(async move {
            // Layer-shell bars have no xdg_toplevel parent handle. Exporting one
            // for a native dialog can disconnect the bar's Wayland connection.
            let path = match dialog.select_folder_future(None::<&gtk::Window>).await {
                Ok(file) => file.path(),
                Err(error) => {
                    if !error.matches(gtk::DialogError::Dismissed)
                        && !error.matches(gtk::DialogError::Cancelled)
                    {
                        controller.error(&error.to_string());
                    }
                    controller.active.set(false);
                    controller.sensitive(true);
                    return;
                }
            };
            let Some(path) = path else {
                controller.error("Choose a local directory.");
                controller.active.set(false);
                controller.sensitive(true);
                return;
            };
            let (sender, receiver) = async_channel::bounded(1);
            relm4::spawn(async move {
                let result = bind_workspace(workspace, &path.to_string_lossy(), initialize).await;
                let _ = sender.send(result.map_err(|e| e.to_string())).await;
            });
            if let Ok(Err(error)) = receiver.recv().await {
                controller.error(&error);
            }
            controller.active.set(false);
            controller.sensitive(true);
        });
    }

    fn sensitive(&self, sensitive: bool) {
        if let Some(button) = self.button.upgrade() {
            button.set_sensitive(sensitive);
        }
    }

    fn parent(&self) -> Option<gtk::Window> {
        self.button.upgrade()?.root().and_downcast::<gtk::Window>()
    }

    fn error(&self, message: &str) {
        let Some(_parent) = self.parent() else {
            return;
        };
        let dialog = gtk::AlertDialog::builder()
            .modal(true)
            .message("Could not associate project")
            .detail(message)
            .build();
        dialog.show(None::<&gtk::Window>);
    }
}
