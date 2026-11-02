use super::Layout;
use crate::{WorkspaceInput, model::Navigate};
use adw::prelude::*;
use relm4::Sender;

pub(super) fn bind(ui: &Layout, sender: Sender<WorkspaceInput>) {
    let tx = sender.clone();
    let guard = ui.rendering.clone();
    ui.project.connect_selected_notify(move |dropdown| {
        if !guard.get() {
            tx.emit(WorkspaceInput::ProjectIndex(dropdown.selected()));
        }
    });
    let tx = sender.clone();
    let guard = ui.rendering.clone();
    ui.tasks.connect_row_selected(move |_, row| {
        if !guard.get() {
            if let Some(row) = row {
                let id = row.widget_name().to_string();
                tx.emit(WorkspaceInput::Navigate(Navigate::Task(
                    (!id.is_empty()).then_some(id),
                )));
            }
        }
    });
    let tx = sender.clone();
    let guard = ui.rendering.clone();
    ui.notes.connect_row_selected(move |_, row| {
        if !guard.get() {
            if let Some(row) = row {
                tx.emit(WorkspaceInput::Navigate(Navigate::Note(
                    row.widget_name().as_str().into(),
                )));
            }
        }
    });
    let tx = sender.clone();
    ui.window.connect_close_request(move |_| {
        tx.emit(WorkspaceInput::Navigate(Navigate::Close));
        gtk::glib::Propagation::Stop
    });
    let shortcuts = gtk::ShortcutController::new();
    shortcuts.set_scope(gtk::ShortcutScope::Managed);
    for (key, which) in [
        ("<Control>s", 0),
        ("<Control>n", 1),
        ("<Control>w", 2),
        ("Escape", 2),
    ] {
        let tx = sender.clone();
        let action = gtk::CallbackAction::new(move |_, _| {
            tx.emit(match which {
                0 => WorkspaceInput::Save,
                1 => WorkspaceInput::Navigate(Navigate::New),
                _ => WorkspaceInput::Navigate(Navigate::Close),
            });
            gtk::glib::Propagation::Stop
        });
        shortcuts.add_shortcut(gtk::Shortcut::new(
            gtk::ShortcutTrigger::parse_string(key),
            Some(action),
        ));
    }
    ui.window.add_controller(shortcuts);
}
