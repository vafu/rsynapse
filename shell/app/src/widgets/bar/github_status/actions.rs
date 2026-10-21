use shell_core::source::github;

use super::{launch, source::PrEntry};

pub(super) async fn open_pr(entry: PrEntry) -> Result<(), String> {
    launch::open(&entry.url).await?;
    mark_seen(entry).await
}

pub(super) async fn mark_seen(entry: PrEntry) -> Result<(), String> {
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
        .call("MarkSeen", &(vec![(entry.url, entry.revision)],))
        .await
        .map_err(|error| format!("could not mark PR seen: {error}"))?;
    Ok(())
}
