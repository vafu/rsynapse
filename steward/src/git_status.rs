//! Refresh trigger only. Projd owns git metadata and its reactive properties.
use crate::relations::{self, LocusClient};
use futures_util::StreamExt;
use shell_source::rx::Observable as _;
use zbus::Proxy;
pub struct GitStatus {
    locus: LocusClient,
}
impl GitStatus {
    pub fn new(locus: LocusClient) -> Self {
        Self { locus }
    }
    pub async fn run(self) -> anyhow::Result<()> {
        let conn = zbus::Connection::session().await?;
        let proxy = Proxy::new(
            &conn,
            goal_model::BUS_NAME,
            goal_model::ROOT_PATH,
            goal_model::MANAGER_INTERFACE,
        )
        .await?;
        let mut changes =
            relations::records(self.locus.clone(), relations::WORKSPACE_PROJECT).into_stream();
        while let Some(Ok(records)) = changes.next().await {
            let mut paths: Vec<_> = records
                .into_iter()
                .filter_map(|r| match r.target {
                    locus::RelationEndpoint::StableKey { kind, id }
                        if kind == locus::keys::PROJECT_PATH =>
                    {
                        Some(id)
                    }
                    _ => None,
                })
                .collect();
            paths.sort();
            paths.dedup();
            for path in paths {
                if let Err(e) = proxy
                    .call::<_, _, goal_model::ProjectInfo>("Refresh", &(path,))
                    .await
                {
                    eprintln!("[steward/project-refresh] {e}");
                }
            }
        }
        Ok(())
    }
}
