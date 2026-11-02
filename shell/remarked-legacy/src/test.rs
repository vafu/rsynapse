//! End-to-end component test against real services on a private bus.
use super::*;
use futures_util::StreamExt;
use shell_core::gtk4_layer_shell::{KeyboardMode, Layer as SurfaceLayer, LayerShell};
use sourceview5::prelude::BufferExt;
use std::{
    cell::Cell,
    process::{Child, Command},
    rc::Rc,
    time::Duration,
};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[track_caller]
fn pump_until(mut ready: impl FnMut() -> bool) {
    let expired = Rc::new(Cell::new(false));
    let timeout = Rc::clone(&expired);
    let watchdog =
        gtk::glib::timeout_add_local_once(Duration::from_secs(20), move || timeout.set(true));
    let context = gtk::glib::MainContext::default();
    while !ready() && !expired.get() {
        context.iteration(true);
    }
    assert!(!expired.get(), "component did not reach the expected state");
    watchdog.remove();
}

#[test]
#[ignore = "requires private D-Bus, a GTK display, and PROJD_BIN/REMARKED_BIN/REMARKED_CLI_BIN"]
fn workspace_ui_roundtrip() {
    assert_eq!(
        std::env::var("RSYNAPSE_UI_TEST").as_deref(),
        Ok("1"),
        "run this test on a private bus"
    );
    assert_eq!(
        std::env::var("NO_AT_BRIDGE").as_deref(),
        Ok("1"),
        "use test-ui.sh so portal activation inherits accessibility isolation"
    );
    let temp = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _proj = Process(
        Command::new(std::env::var("PROJD_BIN").unwrap())
            .env(
                "PROJD_STORE_PATH",
                temp.path().join("state/projects.sqlite3"),
            )
            .spawn()
            .unwrap(),
    );
    let _remarked = Process(
        Command::new(std::env::var("REMARKED_BIN").unwrap())
            .env("REMARKED_NOTEBOOK_DIR", temp.path().join("global"))
            .spawn()
            .unwrap(),
    );
    let (project, instance, note) = runtime.block_on(async {
        let conn = zbus::Connection::session().await.unwrap();
        let bus = zbus::fdo::DBusProxy::new(&conn).await.unwrap();
        let mut changes = bus.receive_name_owner_changed().await.unwrap();
        tokio::time::timeout(Duration::from_secs(20), async {
            while !bus
                .name_has_owner(proj_model::BUS_NAME.try_into().unwrap())
                .await
                .unwrap()
                || !bus
                    .name_has_owner(client::SERVICE.try_into().unwrap())
                    .await
                    .unwrap()
            {
                changes.next().await.unwrap();
            }
        })
        .await
        .unwrap();
        let manager = zbus::Proxy::new(
            &conn,
            proj_model::BUS_NAME,
            proj_model::ROOT_PATH,
            proj_model::MANAGER_INTERFACE,
        )
        .await
        .unwrap();
        let project: proj_model::ProjectInfo = manager
            .call(
                "RegisterProject",
                &(temp.path().to_string_lossy().as_ref(),),
            )
            .await
            .unwrap();
        let _: proj_model::GoalInfo = manager
            .call(
                "CreateGoal",
                &(proj_model::GoalInfo {
                    id: "ui-smoke".into(),
                    title: "Fresh workspace task".into(),
                    kind: "task".into(),
                    project: project.cwd.clone(),
                    priority: "medium".into(),
                    status: "planned".into(),
                    ..Default::default()
                },),
            )
            .await
            .unwrap();
        let metrics = zbus::Proxy::new(
            &conn,
            proj_model::BUS_NAME,
            proj_model::ROOT_PATH,
            proj_model::METRICS_INTERFACE,
        )
        .await
        .unwrap();
        let identities: Vec<proj_model::GoalIdentityInfo> = metrics
            .call("ListGoalIdentities", &("ui-smoke",))
            .await
            .unwrap();
        let instance = identities[0].instance_id.clone();
        let notebook = temp.path().join(".notes").to_string_lossy().into_owned();
        let notes = client::NotesClient::open(&notebook, vec![model::task_tag(&instance)])
            .await
            .unwrap();
        let id = notes
            .capture("# First finding\n\nOriginal text.\n")
            .await
            .unwrap();
        (project, instance, id)
    });

    let _runtime_guard = runtime.enter();
    adw::init().unwrap();
    let application = relm4::main_application();
    application
        .register(None::<&gtk::gio::Cancellable>)
        .unwrap();
    let builder = WorkspaceWindow::builder();
    application.add_window(&builder.root);
    let controller = builder.launch(Some(project.id.clone())).detach();
    assert!(controller.widget().is_layer_window());
    assert_eq!(controller.widget().layer(), SurfaceLayer::Overlay);
    assert_eq!(
        controller.widget().namespace().as_deref(),
        Some("rsynapse-workspace")
    );
    assert_eq!(controller.widget().exclusive_zone(), 0);
    assert_eq!(controller.widget().keyboard_mode(), KeyboardMode::None);
    controller.emit(WorkspaceInput::Open(None));
    pump_until(|| {
        controller.widget().is_visible()
            && controller.widget().keyboard_mode() == KeyboardMode::Exclusive
    });
    pump_until(|| matches!(&controller.model().notes, Notes::Ready(_, notes) if notes.len() == 1));
    controller.emit(WorkspaceInput::Navigate(Navigate::Task(Some(
        instance.clone(),
    ))));
    pump_until(|| controller.model().state.task.as_ref() == Some(&instance));
    controller.emit(WorkspaceInput::Navigate(Navigate::Note(note.clone())));
    pump_until(|| controller.model().state.loaded.is_some() && !controller.model().state.busy);
    assert_eq!(
        controller.model().state.loaded.as_ref().unwrap().text,
        "# First finding\n\nOriginal text.\n"
    );

    let buffer = controller.model().ui.editor.buffer();
    let source_buffer = buffer.clone().downcast::<sourceview5::Buffer>().unwrap();
    assert_eq!(source_buffer.language().unwrap().id().as_str(), "markdown");
    assert!(
        !buffer.can_undo(),
        "opening a note must not expose another document's undo history"
    );
    buffer.set_text("# Edited finding\n\nNative Markdown editing.\n");
    pump_until(|| controller.model().state.dirty);
    controller.emit(WorkspaceInput::Navigate(Navigate::Task(None)));
    pump_until(|| controller.widget().visible_dialog().is_some());
    assert_eq!(controller.model().state.task.as_ref(), Some(&instance));
    controller.widget().visible_dialog().unwrap().close();
    controller.emit(WorkspaceInput::Save);
    pump_until(|| !controller.model().state.dirty && !controller.model().state.busy);
    assert!(!controller.model().ui.banner.is_revealed(), "save failed");
    runtime.block_on(async {
        let notes =
            client::NotesClient::open(&temp.path().join(".notes").to_string_lossy(), Vec::new())
                .await
                .unwrap();
        assert_eq!(
            notes.body(&note).await.unwrap().text,
            "# Edited finding\n\nNative Markdown editing.\n"
        );
    });

    controller.emit(WorkspaceInput::Navigate(Navigate::New));
    pump_until(|| controller.model().state.new_note);
    controller
        .model()
        .ui
        .editor
        .buffer()
        .set_text("# Second finding\n\nCreated in the shell.\n");
    controller.emit(WorkspaceInput::Save);
    pump_until(|| !controller.model().state.new_note && !controller.model().state.busy);
    assert_ne!(controller.model().state.selected_note.as_ref(), Some(&note));

    let capture = Command::new(std::env::var("REMARKED_CLI_BIN").unwrap())
        .args([
            "note",
            "--project",
            &project.cwd,
            "--task",
            "ui-smoke",
            "capture",
            "# CLI finding\n\nCaptured by an agent.",
        ])
        .output()
        .unwrap();
    assert!(
        capture.status.success(),
        "{}",
        String::from_utf8_lossy(&capture.stderr)
    );
    pump_until(|| matches!(&controller.model().notes, Notes::Ready(_, notes) if notes.len() == 3));
    assert!(
        std::fs::read_to_string(
            temp.path()
                .join(".notes")
                .join(String::from_utf8_lossy(&capture.stdout).trim())
        )
        .unwrap()
        .contains(&model::task_tag(&instance))
    );

    controller.emit(WorkspaceInput::Import);
    pump_until(|| {
        !controller.model().state.busy
            && controller
                .model()
                .state
                .loaded
                .as_ref()
                .is_some_and(|body| body.tags.iter().any(|tag| tag == "source/projd-context"))
    });
    let imported = controller.model().state.selected_note.clone();
    let buffer = controller.model().ui.editor.buffer();
    assert!(
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .contains("Fresh workspace task")
    );
    pump_until(|| matches!(&controller.model().notes, Notes::Ready(_, notes) if notes.len() == 4));
    controller.emit(WorkspaceInput::Import);
    pump_until(|| controller.model().state.busy);
    pump_until(|| !controller.model().state.busy);
    assert_eq!(controller.model().state.selected_note, imported);
    assert!(matches!(&controller.model().notes, Notes::Ready(_, notes) if notes.len() == 4));

    controller.emit(WorkspaceInput::Start);
    pump_until(|| {
        controller
            .model()
            .selected_task()
            .is_some_and(|task| task.goal.status == "in-progress")
            && matches!(&controller.model().catalog, Catalog::Ready { active, .. } if active.contains(&instance))
            && !controller.model().state.busy
    });
    controller.model().ui.status.set_selected(4);
    pump_until(|| {
        controller
            .model()
            .selected_task()
            .is_some_and(|task| task.goal.status == "cancelled")
            && matches!(&controller.model().catalog, Catalog::Ready { active, .. } if !active.contains(&instance))
            && !controller.model().state.busy
    });
    assert_eq!(controller.model().ui.status.selected(), 4);
    assert!(!controller.model().ui.start.is_sensitive());
    assert!(controller.model().ui.tasks.row_at_index(1).is_none());
    controller.model().ui.finished.set_active(true);
    pump_until(|| controller.model().ui.tasks.row_at_index(1).is_some());
    assert_eq!(
        controller
            .model()
            .ui
            .tasks
            .row_at_index(1)
            .unwrap()
            .widget_name()
            .as_str(),
        instance
    );
    assert!(matches!(&controller.model().notes, Notes::Ready(_, notes) if notes.len() == 4));
    controller.emit(WorkspaceInput::Hide);
    pump_until(|| !controller.widget().is_visible());
    assert_eq!(controller.widget().keyboard_mode(), KeyboardMode::None);
    application.remove_window(controller.widget());
}
