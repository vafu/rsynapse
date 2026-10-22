use super::super::super::ProjectView;
use super::*;
use shell_core::source;

fn view(workspace: &str, project: Option<&str>) -> SelectedWorkspaceView {
    SelectedWorkspaceView {
        workspace_id: Some(workspace.into()),
        project: project.map(|id| ProjectView {
            project_id: Some(id.into()),
            ..ProjectView::default()
        }),
        ..SelectedWorkspaceView::default()
    }
}

struct Unsubscribed(async_channel::Sender<()>);
impl Drop for Unsubscribed {
    fn drop(&mut self) {
        let _ = self.0.try_send(());
    }
}

// This fixture owns no bus connection. Its drop notification proves that the
// editor actually releases its upstream, rather than merely hiding old rows.
fn watched() -> (
    Observable<Option<Vec<String>>>,
    async_channel::Receiver<()>,
    async_channel::Receiver<()>,
) {
    let (sender, receiver) = async_channel::bounded(1);
    let (started, subscribed) = async_channel::bounded(1);
    let source = source::from_task(move |updates| {
        let sender = sender.clone();
        let started = started.clone();
        async move {
            let _updates = updates;
            let _guard = Unsubscribed(sender);
            started.send(()).await.unwrap();
            std::future::pending::<()>().await;
        }
    });
    (source, subscribed, receiver)
}

#[test]
fn editor_reassociation_controls_and_lifetime() {
    // GTK must have one owning test thread, independent of other widget tests.
    if std::env::var_os("SHELL_LABEL_EDITOR_TEST_WORKER").is_none() {
        let qualified = concat!(
            module_path!(),
            "::editor_reassociation_controls_and_lifetime"
        );
        let test_name = qualified.split_once("::").unwrap().1;
        // With an explicit Nix loader, current_exe() points at ld-linux, while
        // argv[0] is the actual test binary. Preserve Cargo's runtime runner.
        let binary = std::env::args_os().next().unwrap();
        let mut command =
            if let Ok(runner) = std::env::var("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER") {
                let args = shell_words::split(&runner).unwrap();
                let mut command = std::process::Command::new(&args[0]);
                command.args(&args[1..]).arg(&binary);
                command
            } else {
                std::process::Command::new(binary)
            };
        assert!(
            command
                .args(["--exact", test_name, "--nocapture"])
                .env("SHELL_LABEL_EDITOR_TEST_WORKER", "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    adw::init().expect("GTK display required for label-editor integration test");
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _runtime = runtime.enter();
    let context = gtk::glib::MainContext::default();
    let _owner = context.acquire().unwrap();
    context.block_on(async {
        let overlay = adw::ToastOverlay::new();
        let actions = Actions::new(&overlay);
        let editor = LabelEditor::new(&actions);
        assert!(!editor.widget().is_visible());

        let (first_source, first_started, first_dropped) = watched();
        editor.retarget(&view("w", Some("a")), |_| first_source);
        first_started.recv().await.unwrap();
        let first = editor.0.target.borrow().clone().unwrap();
        assert!(editor.widget().is_visible());
        assert!(!editor.0.entry.is_sensitive());
        assert_eq!(editor.0.status.text(), "Labels loading or unavailable");

        // Deliver authoritative property snapshots without a live bus.
        editor.0.receive(&first, Some(vec![]));
        assert!(editor.0.entry.is_sensitive());
        assert!(editor.0.status.text().contains("showing all"));
        assert!(!editor.0.add.is_sensitive());
        editor.0.entry.set_text(" \t ");
        editor.0.entry.emit_activate();
        editor.0.add.emit_clicked();
        assert!(editor.0.command.borrow().is_none());

        editor.0.entry.set_text(" needs review ");
        assert!(editor.0.add.is_sensitive());
        editor.0.receive(&first, Some(vec!["needs review".into()]));
        assert!(!editor.0.add.is_sensitive());
        editor.0.entry.emit_activate();
        assert!(editor.0.command.borrow().is_none());

        // Failures retain the original input, and success waits for properties
        // rather than installing an action's potentially stale response.
        editor.0.entry.set_text("new label");
        editor.0.busy.set(true);
        editor.0.update_controls();
        assert!(!editor.0.add.is_sensitive());
        assert!(editor.0.finish(&first, Some("new label"), false));
        assert_eq!(editor.0.entry.text(), "new label");
        assert!(editor.0.add.is_sensitive());
        assert!(editor.0.finish(&first, Some("new label"), true));
        assert_eq!(editor.0.entry.text(), "");
        assert_eq!(
            editor.0.labels.borrow().as_deref(),
            Some(["needs review".into()].as_slice())
        );
        editor.0.entry.set_text("newer input");
        assert!(editor.0.finish(&first, Some("previous input"), true));
        assert_eq!(editor.0.entry.text(), "newer input");

        // Metadata refreshes of the same association preserve input/subscription.
        editor.0.entry.set_text("draft");
        editor.retarget(&view("w", Some("a")), |_| {
            panic!("same target resubscribed")
        });
        assert_eq!(editor.0.entry.text(), "draft");

        let (second_source, second_started, second_dropped) = watched();
        editor.retarget(&view("w", Some("b")), |_| second_source);
        second_started.recv().await.unwrap();
        let second = editor.0.target.borrow().clone().unwrap();
        assert_ne!(first, second);
        assert_eq!(editor.0.entry.text(), "");
        assert!(editor.0.labels.borrow().is_none());
        first_dropped.recv().await.unwrap();

        editor.0.receive(&first, Some(vec!["stale".into()]));
        assert!(editor.0.labels.borrow().is_none());
        editor.0.remove_label(&first, "needs review");
        assert!(editor.0.command.borrow().is_none());
        editor.0.entry.set_text("b draft");
        editor.0.busy.set(true);
        assert!(!editor.0.finish(&first, Some("b draft"), true));
        assert_eq!(editor.0.entry.text(), "b draft");
        assert!(editor.0.busy.get());

        // Leaving and returning to the same project still invalidates old work.
        editor.retarget(&view("w", Some("a")), |_| source::once(None));
        let returned = editor.0.target.borrow().clone().unwrap();
        assert_ne!(first.generation, returned.generation);
        assert!(!editor.0.finish(&first, None, false));
        second_dropped.recv().await.unwrap();
        editor.0.receive(&returned, Some(vec![]));
        editor.0.entry.set_text("keep on unavailable");
        editor.0.receive(&returned, None);
        assert_eq!(editor.0.entry.text(), "keep on unavailable");
        assert!(!editor.0.entry.is_sensitive());
        assert!(!editor.0.status.text().contains("showing all"));

        // A workspace switch resets drafts even if the project ID is unchanged.
        editor.retarget(&view("other", Some("a")), |_| source::once(None));
        assert_eq!(editor.0.entry.text(), "");
        assert!(!editor.0.finish(&returned, None, true));
        editor.retarget(&view("other", None), |_| panic!("unassigned source"));
        assert!(!editor.widget().is_visible());
        assert!(editor.0.watch.borrow().is_none());

        let (last_source, last_started, last_dropped) = watched();
        editor.retarget(&view("w", Some("c")), |_| last_source);
        last_started.recv().await.unwrap();
        let state = Rc::downgrade(&editor.0);
        let widget = editor.widget().downgrade();
        drop(editor);
        assert!(state.upgrade().is_none());
        assert!(widget.upgrade().is_none());
        last_dropped.recv().await.unwrap();
    });
}
