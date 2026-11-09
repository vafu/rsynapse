use shell_core::source::github;

use super::launch;

pub(super) async fn open_pr(url: String, revision: String) -> Result<(), String> {
    launch::open(&url).await?;
    mark_seen(url, revision).await
}

pub(super) async fn mark_seen(url: String, revision: String) -> Result<(), String> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|error| error.to_string())?;
    let proxy = zbus::Proxy::new(
        &connection,
        github::BUS_NAME,
        github::PR_PATH,
        github::BUS_NAME,
    )
    .await
    .map_err(|error| error.to_string())?;
    let _: u32 = proxy
        .call("MarkSeen", &(vec![(url, revision)],))
        .await
        .map_err(|error| format!("could not mark PR seen: {error}"))?;
    Ok(())
}
