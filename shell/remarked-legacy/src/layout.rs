use super::{WorkspaceInput, model::Navigate};
use adw::prelude::*;
use relm4::Sender;
mod connections;
mod editor;
mod helpers;
use helpers::{button, list, scroll};
use std::{cell::Cell, rc::Rc};

pub(crate) struct Layout {
    pub window: adw::ApplicationWindow,
    pub project: gtk::DropDown,
    pub project_title: adw::WindowTitle,
    pub tasks: gtk::ListBox,
    pub notes: gtk::ListBox,
    pub editor: sourceview5::View,
    pub tags: gtk::Entry,
    pub title: adw::WindowTitle,
    pub notes_title: gtk::Label,
    pub task_description: gtk::Label,
    pub save: gtk::Button,
    pub reload: gtk::Button,
    pub start: gtk::Button,
    pub status: gtk::DropDown,
    pub import: gtk::Button,
    pub banner: adw::Banner,
    pub toast: adw::ToastOverlay,
    pub editor_stack: gtk::Stack,
    pub navigation: adw::NavigationSplitView,
    pub finished: gtk::CheckButton,
    pub rendering: Rc<Cell<bool>>,
}

impl Layout {
    pub fn new(root: &adw::ApplicationWindow, sender: Sender<WorkspaceInput>) -> Self {
        let rendering = Rc::new(Cell::new(false));
        let project = gtk::DropDown::new(None::<gtk::StringList>, None::<gtk::Expression>);
        project.set_tooltip_text(Some("Project"));
        let tasks = list();
        let notes = list();
        let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 12);
        sidebar.set_margin_start(12);
        sidebar.set_margin_end(12);
        sidebar.set_margin_top(12);
        sidebar.set_margin_bottom(12);
        sidebar.append(&project);
        let task_bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let task_heading = gtk::Label::new(Some("Tasks"));
        task_heading.add_css_class("heading");
        task_heading.set_hexpand(true);
        task_heading.set_halign(gtk::Align::Start);
        task_bar.append(&task_heading);
        task_bar.append(&button(
            "list-add-symbolic",
            "New task",
            sender.clone(),
            || WorkspaceInput::TaskDialog {
                subtask: false,
                edit: false,
            },
        ));
        task_bar.append(&button(
            "view-list-symbolic",
            "New subtask",
            sender.clone(),
            || WorkspaceInput::TaskDialog {
                subtask: true,
                edit: false,
            },
        ));
        sidebar.append(&task_bar);
        sidebar.append(&scroll(&tasks));
        let finished = gtk::CheckButton::with_label("Show finished tasks");
        let tx = sender.clone();
        finished.connect_toggled(move |_| tx.emit(WorkspaceInput::Render));
        sidebar.append(&finished);
        let stop = gtk::Button::with_label("Stop tracking");
        stop.add_css_class("flat");
        let tx = sender.clone();
        stop.connect_clicked(move |_| tx.emit(WorkspaceInput::StopTracking));
        sidebar.append(&stop);

        let note_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let note_bar = adw::HeaderBar::new();
        note_bar.set_show_start_title_buttons(false);
        note_bar.set_show_end_title_buttons(false);
        let notes_title = gtk::Label::new(Some("Project notes"));
        notes_title.add_css_class("heading");
        note_bar.set_title_widget(Some(&notes_title));
        note_bar.pack_end(&button(
            "document-new-symbolic",
            "New note (Ctrl+N)",
            sender.clone(),
            || WorkspaceInput::Navigate(Navigate::New),
        ));
        note_box.append(&note_bar);
        let task_description = gtk::Label::new(None);
        task_description.set_wrap(true);
        task_description.set_xalign(0.0);
        task_description.set_margin_start(12);
        task_description.set_margin_end(12);
        task_description.set_selectable(true);
        task_description.add_css_class("dim-label");
        note_box.append(&task_description);
        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        controls.set_margin_start(12);
        controls.set_margin_end(12);
        controls.set_margin_bottom(8);
        let start = gtk::Button::with_label("Start");
        start.add_css_class("suggested-action");
        let tx = sender.clone();
        start.connect_clicked(move |_| tx.emit(WorkspaceInput::Start));
        let status = gtk::DropDown::from_strings(&[
            "Planned",
            "In progress",
            "Completed",
            "Deferred",
            "Cancelled",
        ]);
        let tx = sender.clone();
        let guard = rendering.clone();
        status.connect_selected_notify(move |v| {
            if !guard.get() {
                tx.emit(WorkspaceInput::Status(v.selected()));
            }
        });
        controls.append(&start);
        controls.append(&status);
        controls.append(&button(
            "document-edit-symbolic",
            "Edit task",
            sender.clone(),
            || WorkspaceInput::TaskDialog {
                subtask: false,
                edit: true,
            },
        ));
        note_box.append(&controls);
        let search = gtk::SearchEntry::builder()
            .placeholder_text("Filter notes by title or tag")
            .build();
        search.set_margin_start(12);
        search.set_margin_end(12);
        search.set_margin_bottom(8);
        let tx = sender.clone();
        search.connect_search_changed(move |entry| {
            tx.emit(WorkspaceInput::Filter(entry.text().to_string()))
        });
        note_box.append(&search);
        note_box.append(&scroll(&notes));
        let import = gtk::Button::with_label("Import task context");
        import.set_margin_start(12);
        import.set_margin_end(12);
        import.set_margin_bottom(12);
        let tx = sender.clone();
        import.connect_clicked(move |_| tx.emit(WorkspaceInput::Import));
        note_box.append(&import);

        let editor::Editor {
            container: editor_box,
            view: editor,
            tags,
            title,
            save,
            reload,
            stack: editor_stack,
        } = editor::build(sender.clone(), rendering.clone());
        let note_page = adw::NavigationPage::new(&note_box, "Notes");
        let editor_page = adw::NavigationPage::new(&editor_box, "Editor");
        let navigation = adw::NavigationSplitView::builder()
            .sidebar(&note_page)
            .content(&editor_page)
            .min_sidebar_width(230.0)
            .max_sidebar_width(320.0)
            .build();
        let split = adw::OverlaySplitView::builder()
            .sidebar(&sidebar)
            .content(&navigation)
            .min_sidebar_width(240.0)
            .max_sidebar_width(320.0)
            .build();
        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        header.set_show_start_title_buttons(false);
        header.set_show_end_title_buttons(false);
        let project_title = adw::WindowTitle::new("Tasks & Notes", "");
        header.set_title_widget(Some(&project_title));
        let sidebar_button = gtk::ToggleButton::builder()
            .icon_name("sidebar-show-symbolic")
            .active(true)
            .build();
        sidebar_button.set_tooltip_text(Some("Toggle task sidebar"));
        sidebar_button
            .bind_property("active", &split, "show-sidebar")
            .bidirectional()
            .sync_create()
            .build();
        header.pack_start(&sidebar_button);
        header.pack_end(&button(
            "window-close-symbolic",
            "Close (Esc)",
            sender.clone(),
            || WorkspaceInput::Navigate(Navigate::Close),
        ));
        header.pack_end(&button(
            "view-refresh-symbolic",
            "Retry notebook connection",
            sender.clone(),
            || WorkspaceInput::Retry,
        ));
        toolbar.add_top_bar(&header);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let banner = adw::Banner::new("");
        content.append(&banner);
        split.set_vexpand(true);
        content.append(&split);
        toolbar.set_content(Some(&content));
        let toast = adw::ToastOverlay::new();
        toast.set_child(Some(&toolbar));
        root.set_content(Some(&toast));
        let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            1000.0,
            adw::LengthUnit::Sp,
        ));
        breakpoint.add_setter(&split, "collapsed", Some(&true.to_value()));
        root.add_breakpoint(breakpoint);
        let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            650.0,
            adw::LengthUnit::Sp,
        ));
        breakpoint.add_setter(&navigation, "collapsed", Some(&true.to_value()));
        root.add_breakpoint(breakpoint);

        let layout = Self {
            window: root.clone(),
            project,
            project_title,
            tasks,
            notes,
            editor,
            tags,
            title,
            notes_title,
            task_description,
            save,
            reload,
            start,
            status,
            import,
            banner,
            toast,
            editor_stack,
            navigation,
            finished,
            rendering,
        };
        connections::bind(&layout, sender);
        layout
    }
}
