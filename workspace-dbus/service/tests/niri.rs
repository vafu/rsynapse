use futures_util::StreamExt;
use niri_ipc::Event;
use std::{process::Command, time::Duration};
use workspace_dbus_api::{
    BUS_NAME, ManagerProxy, OutputProxy, ROOT_PATH, WindowProxy, WorkspaceProxy, output_path,
    window_path, workspace_path,
};
use zbus::{
    Connection,
    fdo::{DBusProxy, ObjectManagerProxy},
};
mod support;

#[test]
fn niri_socket_projects_the_neutral_api_and_reconnects() {
    if std::env::var_os("WORKSPACE_NIRI_WORKER").is_none() {
        let status = Command::new("dbus-run-session")
            .arg("--")
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "niri_socket_projects_the_neutral_api_and_reconnects",
                "--nocapture",
            ])
            .env("WORKSPACE_NIRI_WORKER", "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        tokio::time::timeout(Duration::from_secs(15), exercise())
            .await
            .expect("projection stopped making progress")
            .unwrap();
    });
}

async fn exercise() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("niri.sock");
    let source = support::Source::start(tokio::net::UnixListener::bind(&path)?);
    let connection = Connection::session().await?;
    let bus = DBusProxy::new(&connection).await?;
    let mut owners = bus.receive_name_owner_changed().await?;
    let host = tokio::spawn(workspace_dbus::run(workspace_dbus::Config {
        backend: workspace_dbus::BackendKind::Niri,
        socket: Some(path),
    }));
    loop {
        let message = owners.next().await.unwrap();
        let args = message.args()?;
        if args.name().as_str() == BUS_NAME && args.new_owner().is_some() {
            break;
        }
    }
    let manager = ManagerProxy::new(&connection).await?;
    let mut connections = manager.receive_connected_changed().await;
    if !manager.connected().await? {
        while !connections.next().await.unwrap().get().await? {}
    }
    let generation = manager.generation().await?;
    let window = WindowProxy::builder(&connection)
        .path(window_path("12"))?
        .build()
        .await?;
    let workspace = WorkspaceProxy::builder(&connection)
        .path(workspace_path("5"))?
        .build()
        .await?;
    let output = OutputProxy::builder(&connection)
        .path(output_path("eDP-1"))?
        .build()
        .await?;
    assert_eq!(window.id().await?, "12");
    assert_eq!(workspace.id().await?, "5");
    assert_eq!(workspace.index().await?, 2);
    assert_eq!(
        workspace.layout().await?,
        workspace_dbus_api::Layout::Scrolling
    );
    assert_eq!(window.column_index().await?, [0]);
    assert_eq!(window.row_index().await?, [0]);
    assert!(window.pid().await?.is_empty());
    assert_eq!(window.workspace().await?, [workspace_path("5")]);
    assert_eq!(output.current_workspace().await?, [workspace_path("5")]);
    assert_eq!(manager.focused_window().await?, [window_path("12")]);
    let objects = ObjectManagerProxy::builder(&connection)
        .destination(BUS_NAME)?
        .path(ROOT_PATH)?
        .build()
        .await?;
    // Every published reference resolves after bootstrap, including the
    // workspace/window cycle, despite native windows arriving first.
    let inventory = objects.get_managed_objects().await?;
    for path in manager
        .outputs()
        .await?
        .into_iter()
        .chain(manager.workspaces().await?)
        .chain(manager.windows().await?)
    {
        assert!(inventory.contains_key(&path));
    }

    let mut titles = window.receive_title_changed().await;
    source.events.send(Event::WindowOpenedOrChanged {
        window: support::window("live title"),
    })?;
    assert_eq!(titles.next().await.unwrap().get().await?, ["live title"]);

    let mut focus = manager.receive_focused_window_changed().await;
    source.events.send(Event::WindowFocusChanged { id: None })?;
    assert!(focus.next().await.unwrap().get().await?.is_empty());
    assert_eq!(workspace.active_window().await?, [window_path("12")]);
    assert_eq!(manager.focused_workspace().await?, [workspace_path("5")]);
    source
        .events
        .send(Event::WindowFocusChanged { id: Some(12) })?;
    assert_eq!(
        focus.next().await.unwrap().get().await?,
        [window_path("12")]
    );

    // Moving the workspace to a newly discovered output updates dependent
    // window.Output too, even though the native window itself did not change.
    let mut outputs = window.receive_output_changed().await;
    *source.outputs.write().await =
        std::collections::HashMap::from([("DP-2".into(), support::output("DP-2"))]);
    source.events.send(Event::WorkspacesChanged {
        workspaces: vec![support::workspace("DP-2")],
    })?;
    assert_eq!(
        outputs.next().await.unwrap().get().await?,
        [output_path("DP-2")]
    );

    let mut removed = objects.receive_interfaces_removed().await?;
    let mut workspaces = manager.receive_workspaces_changed().await;
    let mut membership = window.receive_workspace_changed().await;
    source
        .events
        .send(Event::WorkspacesChanged { workspaces: vec![] })?;
    while !workspaces.next().await.unwrap().get().await?.is_empty() {}
    assert!(membership.next().await.unwrap().get().await?.is_empty());
    loop {
        let message = removed.next().await.unwrap();
        if message.args()?.object_path().as_str() == workspace_path("5").as_str() {
            break;
        }
    }
    assert!(window.workspace().await?.is_empty());
    assert!(
        !objects
            .get_managed_objects()
            .await?
            .contains_key(&workspace_path("5"))
    );

    *source.outputs.write().await =
        std::collections::HashMap::from([("eDP-1".into(), support::output("eDP-1"))]);
    source.disconnect.send_replace(true);
    while connections.next().await.unwrap().get().await? {}
    assert!(!manager.connected().await?);
    // Release the native source while the host is in transport retry backoff.
    source.disconnect.send_replace(false);
    while !connections.next().await.unwrap().get().await? {}
    assert!(manager.generation().await? > generation);
    assert_eq!(manager.workspaces().await?, [workspace_path("5")]);
    assert_eq!(
        WindowProxy::builder(&connection)
            .path(window_path("12"))?
            .build()
            .await?
            .title()
            .await?,
        ["initial"]
    );
    host.abort();
    Ok(())
}
