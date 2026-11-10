//! Explicit, read-only diagnostic; never part of the normal test suite.
use super::*;

#[tokio::test]
#[ignore = "reads the real session bus; no metrics or domain writes"]
async fn live_focus_probe() {
    let locus = LocusClient::connect().await.unwrap();
    let mut focus = focus_state(&locus).into_stream();
    let connection = zbus::Connection::session().await.unwrap();
    let manager = workspace_dbus_api::ManagerProxy::builder(&connection)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(12), async {
        while let Some(item) = focus.next().await {
            let state = item.unwrap();
            let paths = manager.focused_workspace().await.unwrap();
            let actual = if let Some(path) = paths.first() {
                Some(
                    workspace_dbus_api::WorkspaceProxy::builder(&connection)
                        .path(path.clone())
                        .unwrap()
                        .build()
                        .await
                        .unwrap()
                        .id()
                        .await
                        .unwrap(),
                )
            } else {
                None
            };
            eprintln!("collector={state:?}; current_workspace={actual:?}");
        }
    })
    .await;
}
