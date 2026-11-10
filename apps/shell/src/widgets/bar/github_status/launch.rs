pub(super) async fn open(url: &str) -> Result<(), String> {
    data_sources::write::open_url(url).await
}
