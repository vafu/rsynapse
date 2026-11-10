use crate::{CheckoutObject, Manager, ProjectObject, error, path};
use proj_model::RemovedProjectInfo;
use zbus::Connection;
impl Manager {
    pub(super) async fn delete_project(
        &self,
        id: &str,
        conn: &Connection,
    ) -> zbus::fdo::Result<Option<RemovedProjectInfo>> {
        let _lock = self.mutation.lock().await;
        let Some(removed) = self.store.remove_project(id).map_err(error)? else {
            return Ok(None);
        };
        for c in &removed.checkouts {
            let _ = self.watch.send(crate::watch::Request::Unwatch {
                root: c.root_path.clone(),
            });
            conn.object_server()
                .remove::<CheckoutObject, _>(path("Checkouts", &c.id))
                .await
                .map_err(error)?;
        }
        conn.object_server()
            .remove::<ProjectObject, _>(path("Projects", id))
            .await
            .map_err(error)?;
        Ok(Some(removed.info()))
    }
}
