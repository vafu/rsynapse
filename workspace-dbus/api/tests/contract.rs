use std::{process::Command, time::Duration};

use futures_util::StreamExt;
use workspace_dbus_api::{
    BUS_NAME, MANAGER_INTERFACE, ManagerProxy, OUTPUT_INTERFACE, OutputProxy, ROOT_PATH,
    WINDOW_INTERFACE, WORKSPACE_INTERFACE, WindowProxy, WorkspaceProxy, output_path, window_path,
    workspace_path,
};
use zbus::{
    Connection,
    fdo::{IntrospectableProxy, ObjectManagerProxy},
};

mod support;
use support::{OUTPUT, WINDOW, WORKSPACE};

/// Standard `cargo test` must not acquire the service name on a real desktop.
#[test]
fn wire_contract() {
    if std::env::var_os("WORKSPACE_DBUS_CONTRACT_WORKER").is_none() {
        let status = Command::new("dbus-run-session")
            .arg("--")
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", "wire_contract", "--nocapture"])
            .env("WORKSPACE_DBUS_CONTRACT_WORKER", "1")
            .status()
            .expect("dbus-run-session is required for wire contract verification");
        assert!(
            status.success(),
            "private-bus contract test failed: {status}"
        );
        return;
    }

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        tokio::time::timeout(Duration::from_secs(10), verify_contract())
            .await
            .expect("D-Bus contract test stopped making progress")
            .unwrap();
    });
}

async fn verify_contract() -> zbus::Result<()> {
    let (service, state) = support::serve().await?;
    let client = Connection::session().await?;
    let manager = ManagerProxy::new(&client).await?;
    let output = OutputProxy::builder(&client)
        .path(output_path(OUTPUT))?
        .build()
        .await?;
    let workspace = WorkspaceProxy::builder(&client)
        .path(workspace_path(WORKSPACE))?
        .build()
        .await?;
    let window = WindowProxy::builder(&client)
        .path(window_path(WINDOW))?
        .build()
        .await?;
    let objects = ObjectManagerProxy::builder(&client)
        .destination(BUS_NAME)?
        .path(ROOT_PATH)?
        .build()
        .await?;

    // Read every public property through the API crate, against an independent
    // producer. This verifies serialization, including opaque string IDs and
    // zero-or-one arrays, rather than assuming Rust type declarations match.
    assert!(manager.connected().await?);
    assert_eq!(manager.generation().await?, 1);
    assert_eq!(manager.outputs().await?, [output_path(OUTPUT)]);
    assert_eq!(manager.workspaces().await?, [workspace_path(WORKSPACE)]);
    assert_eq!(manager.windows().await?, [window_path(WINDOW)]);
    assert_eq!(manager.focused_output().await?, [output_path(OUTPUT)]);
    assert_eq!(
        manager.focused_workspace().await?,
        [workspace_path(WORKSPACE)]
    );
    assert_eq!(manager.focused_window().await?, [window_path(WINDOW)]);

    assert_eq!(output.name().await?, OUTPUT);
    assert!(output.focused().await?);
    assert_eq!(
        output.current_workspace().await?,
        [workspace_path(WORKSPACE)]
    );
    assert_eq!(output.workspaces().await?, [workspace_path(WORKSPACE)]);

    assert_eq!(workspace.id().await?, WORKSPACE);
    assert_eq!(workspace.name().await?, ["2"]);
    assert_eq!(workspace.index().await?, 2);
    assert_eq!(workspace.output().await?, [output_path(OUTPUT)]);
    assert!(workspace.active().await?);
    assert!(workspace.focused().await?);
    assert!(!workspace.urgent().await?);
    assert_eq!(workspace.active_window().await?, [window_path(WINDOW)]);
    assert_eq!(workspace.windows().await?, [window_path(WINDOW)]);

    assert_eq!(window.id().await?, WINDOW);
    assert_eq!(window.title().await?, ["initial title"]);
    assert_eq!(window.app_id().await?, ["example.application"]);
    assert!(window.pid().await?.is_empty());
    assert_eq!(window.workspace().await?, [workspace_path(WORKSPACE)]);
    assert_eq!(window.output().await?, [output_path(OUTPUT)]);
    assert!(window.focused().await?);
    assert!(!window.floating().await?);
    assert!(!window.urgent().await?);
    assert_eq!(window.column_index().await?, [0]);
    assert!(window.row_index().await?.is_empty());

    let managed = objects.get_managed_objects().await?;
    for (path, interface) in [
        (output_path(OUTPUT), OUTPUT_INTERFACE),
        (workspace_path(WORKSPACE), WORKSPACE_INTERFACE),
        (window_path(WINDOW), WINDOW_INTERFACE),
    ] {
        let interfaces = managed
            .get(&path)
            .expect("object missing from ObjectManager");
        assert!(interfaces.keys().any(|name| name.as_str() == interface));
    }
    // Inspect the actual wire values so an accidental numeric ID cannot pass.
    let window_properties = &managed[&window_path(WINDOW)]
        .iter()
        .find(|(name, _)| name.as_str() == WINDOW_INTERFACE)
        .unwrap()
        .1;
    assert_eq!(window_properties["Id"].value_signature().as_str(), "s");
    let workspace_properties = &managed[&workspace_path(WORKSPACE)]
        .iter()
        .find(|(name, _)| name.as_str() == WORKSPACE_INTERFACE)
        .unwrap()
        .1;
    assert_eq!(workspace_properties["Id"].value_signature().as_str(), "s");
    assert_eq!(
        workspace_properties["Index"].value_signature().as_str(),
        "u"
    );

    let introspection = IntrospectableProxy::builder(&client)
        .destination(BUS_NAME)?
        .path(ROOT_PATH)?
        .build()
        .await?
        .introspect()
        .await?;
    assert!(introspection.contains(MANAGER_INTERFACE));
    assert!(introspection.contains("org.freedesktop.DBus.ObjectManager"));

    // Title is live in the new contract (the old bridge omitted its signal).
    let mut titles = window.receive_title_changed().await;
    state.write().await.title = "updated title".into();
    let window_ref = service
        .object_server()
        .interface::<_, support::Window>(window_path(WINDOW))
        .await?;
    window_ref
        .get()
        .await
        .title_changed(window_ref.signal_context())
        .await?;
    assert_eq!(titles.next().await.unwrap().get().await?, ["updated title"]);

    // A window may lose global focus while its workspace remembers it.
    let mut focus = manager.receive_focused_window_changed().await;
    let mut window_focus = window.receive_focused_changed().await;
    state.write().await.focused = false;
    window_ref
        .get()
        .await
        .focused_changed(window_ref.signal_context())
        .await?;
    let manager_ref = service
        .object_server()
        .interface::<_, support::Manager>(ROOT_PATH)
        .await?;
    manager_ref
        .get()
        .await
        .focused_window_changed(manager_ref.signal_context())
        .await?;
    assert!(!window_focus.next().await.unwrap().get().await?);
    assert!(focus.next().await.unwrap().get().await?.is_empty());
    assert_eq!(workspace.active_window().await?, [window_path(WINDOW)]);

    let mut removed = objects.receive_interfaces_removed().await?;
    let mut windows = manager.receive_windows_changed().await;
    let mut workspace_windows = workspace.receive_windows_changed().await;
    let mut active_window = workspace.receive_active_window_changed().await;
    state.write().await.window_present = false;
    let workspace_ref = service
        .object_server()
        .interface::<_, support::Workspace>(workspace_path(WORKSPACE))
        .await?;
    workspace_ref
        .get()
        .await
        .windows_changed(workspace_ref.signal_context())
        .await?;
    workspace_ref
        .get()
        .await
        .active_window_changed(workspace_ref.signal_context())
        .await?;
    manager_ref
        .get()
        .await
        .windows_changed(manager_ref.signal_context())
        .await?;
    service
        .object_server()
        .remove::<support::Window, _>(window_path(WINDOW))
        .await?;
    let signal = removed.next().await.unwrap();
    let args = signal.args()?;
    assert_eq!(args.object_path().as_str(), window_path(WINDOW).as_str());
    assert!(args.interfaces().contains(&WINDOW_INTERFACE));
    assert!(windows.next().await.unwrap().get().await?.is_empty());
    // Separate proxy caches process signals independently. Observing the
    // manager's update is not a barrier for the workspace's property streams.
    assert!(
        workspace_windows
            .next()
            .await
            .unwrap()
            .get()
            .await?
            .is_empty()
    );
    assert!(active_window.next().await.unwrap().get().await?.is_empty());
    assert!(
        !objects
            .get_managed_objects()
            .await?
            .contains_key(&window_path(WINDOW))
    );

    let mut connected = manager.receive_connected_changed().await;
    let mut generations = manager.receive_generation_changed().await;
    let mut outputs = manager.receive_outputs_changed().await;
    let mut workspaces = manager.receive_workspaces_changed().await;
    let mut focused_outputs = manager.receive_focused_output_changed().await;
    let mut focused_workspaces = manager.receive_focused_workspace_changed().await;
    {
        let mut state = state.write().await;
        state.connected = false;
        state.generation += 1;
    }
    service
        .object_server()
        .remove::<support::Workspace, _>(workspace_path(WORKSPACE))
        .await?;
    service
        .object_server()
        .remove::<support::Output, _>(output_path(OUTPUT))
        .await?;
    let manager_iface = manager_ref.get().await;
    manager_iface
        .connected_changed(manager_ref.signal_context())
        .await?;
    manager_iface
        .generation_changed(manager_ref.signal_context())
        .await?;
    manager_iface
        .outputs_changed(manager_ref.signal_context())
        .await?;
    manager_iface
        .workspaces_changed(manager_ref.signal_context())
        .await?;
    manager_iface
        .focused_output_changed(manager_ref.signal_context())
        .await?;
    manager_iface
        .focused_workspace_changed(manager_ref.signal_context())
        .await?;
    assert!(!connected.next().await.unwrap().get().await?);
    assert_eq!(generations.next().await.unwrap().get().await?, 2);
    assert!(outputs.next().await.unwrap().get().await?.is_empty());
    assert!(workspaces.next().await.unwrap().get().await?.is_empty());
    assert!(
        focused_outputs
            .next()
            .await
            .unwrap()
            .get()
            .await?
            .is_empty()
    );
    assert!(
        focused_workspaces
            .next()
            .await
            .unwrap()
            .get()
            .await?
            .is_empty()
    );
    assert!(manager.outputs().await?.is_empty());
    assert!(manager.workspaces().await?.is_empty());
    assert!(manager.focused_output().await?.is_empty());
    assert!(manager.focused_workspace().await?.is_empty());
    // zbus retains collection-directory nodes with empty interface maps.
    assert!(
        objects
            .get_managed_objects()
            .await?
            .values()
            .all(|interfaces| interfaces.is_empty())
    );
    Ok(())
}
