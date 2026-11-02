use super::helpers::{button, scroll};
use crate::WorkspaceInput;
use adw::prelude::*;
use relm4::Sender;
use sourceview5::prelude::*;
use std::{cell::Cell, rc::Rc};

pub(super) struct Editor {
    pub container: gtk::Box,
    pub view: sourceview5::View,
    pub tags: gtk::Entry,
    pub title: adw::WindowTitle,
    pub save: gtk::Button,
    pub reload: gtk::Button,
    pub stack: gtk::Stack,
}

pub(super) fn build(sender: Sender<WorkspaceInput>, rendering: Rc<Cell<bool>>) -> Editor {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let bar = adw::HeaderBar::new();
    bar.set_show_start_title_buttons(false);
    bar.set_show_end_title_buttons(false);
    let title = adw::WindowTitle::new("Select a note", "");
    bar.set_title_widget(Some(&title));
    let save = gtk::Button::with_label("Save");
    save.add_css_class("suggested-action");
    save.set_tooltip_text(Some("Save note and tags (Ctrl+S)"));
    let tx = sender.clone();
    save.connect_clicked(move |_| tx.emit(WorkspaceInput::Save));
    let reload = button(
        "view-refresh-symbolic",
        "Reload from notebook",
        sender.clone(),
        || WorkspaceInput::Reload,
    );
    bar.pack_end(&save);
    bar.pack_start(&reload);
    container.append(&bar);
    let tags = gtk::Entry::builder()
        .placeholder_text("Tags, separated by commas")
        .build();
    tags.set_tooltip_text(Some(
        "Tags — task/<immutable-instance-id> associates a note with a task",
    ));
    tags.set_margin_start(18);
    tags.set_margin_end(18);
    tags.set_margin_top(6);
    tags.set_margin_bottom(6);
    let tx = sender.clone();
    let guard = rendering.clone();
    tags.connect_changed(move |_| {
        if !guard.get() {
            tx.emit(WorkspaceInput::Changed);
        }
    });
    container.append(&tags);
    let buffer = sourceview5::Buffer::new(None::<&gtk::TextTagTable>);
    buffer.set_language(
        sourceview5::LanguageManager::default()
            .language("markdown")
            .as_ref(),
    );
    buffer.set_highlight_syntax(true);
    buffer.set_enable_undo(true);
    set_scheme(&buffer);
    let scheme_buffer = buffer.downgrade();
    adw::StyleManager::default().connect_dark_notify(move |_| {
        if let Some(buffer) = scheme_buffer.upgrade() {
            set_scheme(&buffer);
        }
    });
    let view = sourceview5::View::with_buffer(&buffer);
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.set_monospace(true);
    view.set_auto_indent(true);
    view.set_top_margin(18);
    view.set_bottom_margin(24);
    view.set_left_margin(24);
    view.set_right_margin(24);
    let tx = sender.clone();
    let guard = rendering;
    buffer.connect_changed(move |_| {
        if !guard.get() {
            tx.emit(WorkspaceInput::Changed);
        }
    });
    let stack = gtk::Stack::new();
    let empty = adw::StatusPage::builder()
        .icon_name("document-edit-symbolic")
        .title("Your project knowledge")
        .description("Select a note, or create one with Ctrl+N.")
        .build();
    stack.add_named(&empty, Some("empty"));
    stack.add_named(&scroll(&view), Some("editor"));
    stack.set_vexpand(true);
    container.append(&stack);
    Editor {
        container,
        view,
        tags,
        title,
        save,
        reload,
        stack,
    }
}

fn set_scheme(buffer: &sourceview5::Buffer) {
    let name = if adw::StyleManager::default().is_dark() {
        "Adwaita-dark"
    } else {
        "Adwaita"
    };
    buffer.set_style_scheme(
        sourceview5::StyleSchemeManager::default()
            .scheme(name)
            .as_ref(),
    );
}
