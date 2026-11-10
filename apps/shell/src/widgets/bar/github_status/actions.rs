use data_sources::github;

use super::launch;

pub(super) async fn open_pr(url: String, revision: String) -> Result<(), String> {
    launch::open(&url).await?;
    mark_seen(url, revision).await
}

pub(super) async fn mark_seen(url: String, revision: String) -> Result<(), String> {
    github::mark_seen(url, revision).await
}
