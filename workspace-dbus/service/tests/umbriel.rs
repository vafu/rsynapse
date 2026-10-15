use futures_util::StreamExt;
use serde_json::json;
use std::{process::Command, time::Duration};
use workspace_dbus_api::{
    BUS_NAME, ManagerProxy, OutputProxy, ROOT_PATH, WindowProxy, WorkspaceProxy, output_path,
    window_path, workspace_path,
};
use zbus::{
    Connection,
    fdo::{DBusProxy, ObjectManagerProxy},
};
#[path = "support/umbriel.rs"]
mod support;
use support::{OTHER, OTHER_WORKSPACE, WINDOW, WORKSPACE};

#[test]
fn umbriel_snapshots_normalize_focus_identity_and_lifecycle() {
    if std::env::var_os("WORKSPACE_UMBRIEL_WORKER").is_none() {
        let status = Command::new("dbus-run-session")
            .arg("--")
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "umbriel_snapshots_normalize_focus_identity_and_lifecycle",
                "--nocapture",
            ])
            .env("WORKSPACE_UMBRIEL_WORKER", "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        tokio::time::timeout(Duration::from_secs(15), exercise())
            .await
            .expect("Umbriel projection stopped making progress")
            .unwrap();
    });
}

async fn exercise() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("umbriel.sock");
    let source = support::Source::start(tokio::net::UnixListener::bind(&path)?);
    let connection = Connection::session().await?;
    let bus = DBusProxy::new(&connection).await?;
    let mut owners = bus.receive_name_owner_changed().await?;
    let host = tokio::spawn(workspace_dbus::run(workspace_dbus::Config {
        backend: workspace_dbus::BackendKind::Umbriel,
        socket: Some(path),
    }));
    loop {
        let signal = owners.next().await.unwrap();
        let args = signal.args()?;
        if args.name().as_str() == BUS_NAME && args.new_owner().is_some() {
            break;
        }
    }
    let manager = ManagerProxy::new(&connection).await?;
    let mut connections = manager.receive_connected_changed().await;
    source.subscribed.notified().await;
    assert!(!manager.connected().await?);
    assert!(
        manager.windows().await?.is_empty(),
        "partial bootstrap must not escape"
    );
    source.finish_bootstrap();
    while !connections.next().await.unwrap().get().await? {}
    let generation = manager.generation().await?;
    let window = WindowProxy::builder(&connection)
        .path(window_path(WINDOW))?
        .build()
        .await?;
    let other = WindowProxy::builder(&connection)
        .path(window_path(OTHER))?
        .build()
        .await?;
    let workspace = WorkspaceProxy::builder(&connection)
        .path(workspace_path(WORKSPACE))?
        .build()
        .await?;
    let other_workspace = WorkspaceProxy::builder(&connection)
        .path(workspace_path(OTHER_WORKSPACE))?
        .build()
        .await?;
    assert_eq!(window.id().await?, WINDOW);
    assert_eq!(other.id().await?, OTHER);
    assert_eq!(workspace.id().await?, WORKSPACE);
    assert!(workspace.name().await?.is_empty());
    assert_eq!(other_workspace.name().await?, ["1"]);
    assert_eq!(window.pid().await?, [321]);
    assert!(other.pid().await?.is_empty());
    assert_eq!(window.column_index().await?, [0]);
    assert_eq!(window.row_index().await?, [1]);
    assert!(other.column_index().await?.is_empty());
    assert!(workspace.active().await? && other_workspace.active().await?);
    assert!(workspace.focused().await? && !other_workspace.focused().await?);
    assert!(window.focused().await? && !other.focused().await?);
    // Pointer preference is DP-B, but keyboard application focus belongs to DP-A.
    assert_eq!(manager.focused_output().await?, [output_path("DP-A")]);
    assert_eq!(
        manager.focused_workspace().await?,
        [workspace_path(WORKSPACE)]
    );
    assert_eq!(other_workspace.active_window().await?, [window_path(OTHER)]);

    let objects = ObjectManagerProxy::builder(&connection)
        .destination(BUS_NAME)?
        .path(ROOT_PATH)?
        .build()
        .await?;
    assert!(
        objects
            .get_managed_objects()
            .await?
            .contains_key(&window_path(WINDOW))
    );
    let mut titles = window.receive_title_changed().await;
    source.send("future_family", json!({"arbitrary":[1, {"new":true}]}));
    source.send("windows", support::windows("live title", true));
    assert_eq!(titles.next().await.unwrap().get().await?, ["live title"]);
    assert_eq!(manager.generation().await?, generation);
    let mut focus = manager.receive_focused_window_changed().await;
    let mut window_focus = window.receive_focused_changed().await;
    source.send("windows", support::windows("live title", false));
    assert!(!window_focus.next().await.unwrap().get().await?);
    assert!(focus.next().await.unwrap().get().await?.is_empty());
    assert_eq!(workspace.active_window().await?, [window_path(WINDOW)]);
    assert_eq!(manager.focused_output().await?, [output_path("DP-B")]);
    assert_eq!(
        manager.focused_workspace().await?,
        [workspace_path(OTHER_WORKSPACE)]
    );

    let mut output_focus = manager.receive_focused_output_changed().await;
    source.send(
        "layers",
        json!([{"output":"DP-A","mapped":true,"focused":true}]),
    );
    assert_eq!(
        output_focus.next().await.unwrap().get().await?,
        [output_path("DP-A")]
    );
    source.send("windows", support::windows("behind layer", true));
    assert_eq!(titles.next().await.unwrap().get().await?, ["behind layer"]);
    assert!(!window.focused().await?);
    assert!(manager.focused_window().await?.is_empty());
    source.send("layers", json!([]));
    assert!(window_focus.next().await.unwrap().get().await?);
    assert_eq!(
        focus.next().await.unwrap().get().await?,
        [window_path(WINDOW)]
    );
    source.send("overview", json!({"open":true}));
    assert!(focus.next().await.unwrap().get().await?.is_empty());
    source.send("overview", json!({"open":false}));
    assert_eq!(
        focus.next().await.unwrap().get().await?,
        [window_path(WINDOW)]
    );

    let mut membership = window.receive_workspace_changed().await;
    let mut focused_workspace = manager.receive_focused_workspace_changed().await;
    let mut scratchpad = support::windows("scratchpad", true);
    scratchpad[0]["workspace"] = json!("");
    scratchpad[0]["output"] = json!("DP-B");
    scratchpad[0]["floating"] = json!(true);
    scratchpad[0]["column"] = json!(9);
    scratchpad[0]["row"] = json!(9);
    source.send("windows", scratchpad);
    assert!(membership.next().await.unwrap().get().await?.is_empty());
    assert!(
        focused_workspace
            .next()
            .await
            .unwrap()
            .get()
            .await?
            .is_empty()
    );
    assert_eq!(window.output().await?, [output_path("DP-B")]);
    assert!(window.column_index().await?.is_empty());
    assert!(workspace.active_window().await?.is_empty());

    let mut windows = manager.receive_windows_changed().await;
    let mut window_output = window.receive_output_changed().await;
    let mut pending = support::windows("pending parent", true)[0].clone();
    pending["workspace"] = json!("new/output:é");
    pending["output"] = json!("DP-C");
    source.send("windows", json!([pending]));
    assert_eq!(
        windows.next().await.unwrap().get().await?,
        [window_path(WINDOW)]
    );
    assert!(window_output.next().await.unwrap().get().await?.is_empty());
    assert!(window.workspace().await?.is_empty());
    source.send(
        "outputs",
        json!([{"name":"DP-C","focused":true,"active_workspace":"new/output:é"}]),
    );
    assert_eq!(
        window_output.next().await.unwrap().get().await?,
        [output_path("DP-C")]
    );
    source.send(
        "workspaces",
        json!([{"id":"new/output:é","name":"new","named":true,"index":3,
        "output":"DP-C","active":true,"focused":true,"urgent":false,"focused_window":WINDOW}]),
    );
    assert_eq!(
        membership.next().await.unwrap().get().await?,
        [workspace_path("new/output:é")]
    );
    assert_eq!(
        focused_workspace.next().await.unwrap().get().await?,
        [workspace_path("new/output:é")]
    );
    assert_eq!(
        OutputProxy::builder(&connection)
            .path(output_path("DP-C"))?
            .build()
            .await?
            .current_workspace()
            .await?,
        [workspace_path("new/output:é")]
    );

    // Rejected/malformed state must clear live objects and bootstrap from scratch.
    let mut duplicate = support::outputs();
    let repeated = duplicate[0].clone();
    duplicate.as_array_mut().unwrap().push(repeated);
    source.send("outputs", duplicate);
    while connections.next().await.unwrap().get().await? {}
    while !connections.next().await.unwrap().get().await? {}
    assert!(manager.generation().await? > generation);
    assert_eq!(manager.workspaces().await?.len(), 2);
    assert!(
        !objects
            .get_managed_objects()
            .await?
            .contains_key(&workspace_path("new/output:é"))
    );
    let restored = WindowProxy::builder(&connection)
        .path(window_path(WINDOW))?
        .build()
        .await?;
    assert_eq!(restored.title().await?, ["initial"]);

    source.events.send(support::Message::Frame(
        json!({"err":"subscription rejected"}),
    ))?;
    while connections.next().await.unwrap().get().await? {}
    while !connections.next().await.unwrap().get().await? {}
    source.events.send(support::Message::Close)?;
    while connections.next().await.unwrap().get().await? {}
    while !connections.next().await.unwrap().get().await? {}
    assert_eq!(manager.windows().await?.len(), 2);
    host.abort();
    Ok(())
}
