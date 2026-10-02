use super::SelectedWorkspaceView;
use shell_core::{
    gtk::{self, prelude::*},
    gtk4_layer_shell::{KeyboardMode, LayerShell},
};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

pub(in crate::widgets::bar) struct WorkspaceEditor {
    target: Rc<RefCell<Option<(u64, String)>>>,
}

impl WorkspaceEditor {
    pub(in crate::widgets::bar) fn new(button: &gtk::MenuButton) -> Self {
        let target = Rc::new(RefCell::new(None::<(u64, String)>));
        let editing = Rc::new(RefCell::new(None::<u64>));
        let popover = gtk::Popover::new();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        let label = gtk::Label::new(Some("Workspace name"));
        let entry = gtk::Entry::new();
        let error = gtk::Label::new(None);
        error.set_wrap(true);
        error.add_css_class("error");
        let save = gtk::Button::with_label("Save");
        save.add_css_class("suggested-action");
        content.append(&label);
        content.append(&entry);
        content.append(&error);
        content.append(&save);
        popover.set_child(Some(&content));
        button.set_popover(Some(&popover));

        let target_for_open = target.clone();
        let editing_for_open = editing.clone();
        let field = entry.clone();
        let message = error.clone();
        popover.connect_visible_notify(move |popover| {
            let visible = popover.is_visible();
            if let Some(window) = popover.root().and_downcast::<gtk::Window>() {
                window.set_keyboard_mode(if visible {
                    KeyboardMode::Exclusive
                } else {
                    KeyboardMode::None
                });
            }
            if visible {
                let current = target_for_open.borrow().clone();
                *editing_for_open.borrow_mut() = current.as_ref().map(|(id, _)| *id);
                field.set_text(
                    current
                        .as_ref()
                        .map(|(_, name)| name.as_str())
                        .unwrap_or_default(),
                );
                field.select_region(0, -1);
                message.set_text("");
                let field = field.clone();
                gtk::glib::idle_add_local_once(move || {
                    field.grab_focus();
                });
            }
        });

        let field = entry.clone();
        let close = popover.downgrade();
        save.connect_clicked(move |save| {
            let Some(close) = close.upgrade() else {
                return;
            };
            let Some(id) = *editing.borrow() else {
                return;
            };
            let name = field.text().trim().to_owned();
            if name.is_empty() {
                error.set_text("Enter a workspace name.");
                return;
            }
            save.set_sensitive(false);
            let (sender, receiver) = async_channel::bounded(1);
            relm4::spawn(async move {
                let result = save_name(id, name).await.map_err(|e| e.to_string());
                let _ = sender.send(result).await;
            });
            let close = close.clone();
            let error = error.clone();
            let save = save.clone();
            gtk::glib::MainContext::default().spawn_local(async move {
                if let Ok(result) = receiver.recv().await {
                    match result {
                        Ok(()) => close.popdown(),
                        Err(message) => error.set_text(&message),
                    }
                }
                save.set_sensitive(true);
            });
        });
        let activate = save.clone();
        entry.connect_activate(move |_| activate.emit_clicked());
        Self { target }
    }

    pub(in crate::widgets::bar) fn set_target(&self, view: &SelectedWorkspaceView) {
        *self.target.borrow_mut() = view
            .workspace_id
            .map(|id| (id, super::title_label(view).to_owned()));
    }
}

async fn save_name(id: u64, name: String) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    let proxy = locus::RelationsProxy::new(&connection).await?;
    proxy
        .set_one(
            locus::RelationEndpoint::stable_key(locus::keys::NIRI_WORKSPACE_ID, id.to_string()),
            "org.rsynapse.workspace.name",
            locus::RelationEndpoint::stable_key("org.rsynapse.workspace.name", name),
            HashMap::from([("source".to_owned(), "manual".to_owned())]),
        )
        .await?;
    Ok(())
}
