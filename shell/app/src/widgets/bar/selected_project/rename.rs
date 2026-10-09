use super::SelectedWorkspaceView;
use shell_core::gtk::{self, prelude::*};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[derive(Clone)]
pub(super) struct WorkspaceEditor {
    target: Rc<RefCell<Option<(u64, String)>>>,
    content: gtk::Box,
    prepare: Rc<dyn Fn()>,
    popover: gtk::glib::WeakRef<gtk::Popover>,
}

impl WorkspaceEditor {
    pub(super) fn new(popover: &gtk::Popover) -> Self {
        let target = Rc::new(RefCell::new(None::<(u64, String)>));
        let editing = Rc::new(RefCell::new(None::<u64>));
        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let label = gtk::Label::new(Some("Workspace name"));
        let entry = gtk::Entry::new();
        let persist = gtk::CheckButton::with_label("Keep after restart");
        let error = gtk::Label::new(None);
        error.set_wrap(true);
        error.add_css_class("error");
        let save = gtk::Button::with_label("Save");
        save.add_css_class("suggested-action");
        content.append(&label);
        content.append(&entry);
        content.append(&persist);
        content.append(&error);
        content.append(&save);

        let target_for_open = target.clone();
        let editing_for_open = editing.clone();
        let field = entry.clone();
        let message = error.clone();
        let persistence_for_open = persist.clone();
        let save_for_open = save.clone();
        let prepare: Rc<dyn Fn()> = Rc::new(move || {
            let current = target_for_open.borrow().clone();
            if current.is_none() {
                return;
            }
            *editing_for_open.borrow_mut() = current.as_ref().map(|(id, _)| *id);
            field.set_text(
                current
                    .as_ref()
                    .map(|(_, name)| name.as_str())
                    .unwrap_or_default(),
            );
            field.select_region(0, -1);
            message.set_text("");
            persistence_for_open.set_active(false);
            if let Some((id, _)) = current {
                save_for_open.set_sensitive(false);
                let (sender, receiver) = async_channel::bounded(1);
                relm4::spawn(async move {
                    let _ = sender
                        .send(read_persistence(id).await.map_err(|e| e.to_string()))
                        .await;
                });
                let persist = persistence_for_open.clone();
                let save = save_for_open.clone();
                let message = message.clone();
                let editing = editing_for_open.clone();
                gtk::glib::MainContext::default().spawn_local(async move {
                    if let Ok(result) = receiver.recv().await {
                        if *editing.borrow() != Some(id) {
                            return;
                        }
                        match result {
                            Ok(value) => {
                                persist.set_active(value);
                                save.set_sensitive(true);
                            }
                            Err(error) => message.set_text(&error),
                        }
                    }
                });
            }
            let field = field.clone();
            gtk::glib::idle_add_local_once(move || {
                field.grab_focus();
            });
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
            let persist = persist.is_active();
            if name.is_empty() {
                error.set_text("Enter a workspace name.");
                return;
            }
            save.set_sensitive(false);
            let (sender, receiver) = async_channel::bounded(1);
            relm4::spawn(async move {
                let result = save_name(id, name, persist)
                    .await
                    .map_err(|e| e.to_string());
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
        let activate = save.downgrade();
        entry.connect_activate(move |_| {
            if let Some(activate) = activate.upgrade() {
                activate.emit_clicked();
            }
        });
        Self {
            target,
            content,
            prepare,
            popover: popover.downgrade(),
        }
    }

    pub(super) fn widget(&self) -> &gtk::Box {
        &self.content
    }
    pub(super) fn prepare(&self) {
        (self.prepare)();
    }

    pub(super) fn set_target(&self, view: &SelectedWorkspaceView) {
        let target = view
            .workspace_id
            .map(|id| (id, view.name.clone().unwrap_or_else(|| "empty".to_owned())));
        let changed = *self.target.borrow() != target;
        *self.target.borrow_mut() = target;
        if changed
            && self.content.is_mapped()
            && self.popover.upgrade().is_some_and(|p| p.is_visible())
        {
            (self.prepare)();
        }
    }
}

async fn save_name(id: u64, name: String, persist: bool) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    let proxy = locus::RelationsProxy::new(&connection).await?;
    proxy
        .set_one_with_persistence(
            locus::RelationEndpoint::stable_key(locus::keys::NIRI_WORKSPACE_ID, id.to_string()),
            "org.rsynapse.workspace.name",
            locus::RelationEndpoint::stable_key("org.rsynapse.workspace.name", name),
            HashMap::from([("source".to_owned(), "manual".to_owned())]),
            persist,
        )
        .await?;
    Ok(())
}

async fn read_persistence(id: u64) -> zbus::Result<bool> {
    let connection = zbus::Connection::session().await?;
    let proxy = locus::RelationsProxy::new(&connection).await?;
    let subject =
        locus::RelationEndpoint::stable_key(locus::keys::NIRI_WORKSPACE_ID, id.to_string());
    Ok(proxy
        .list_with_persistence("org.rsynapse.workspace.name")
        .await?
        .into_iter()
        .find(|state| state.record.subject == subject)
        .is_some_and(|state| state.persist))
}
