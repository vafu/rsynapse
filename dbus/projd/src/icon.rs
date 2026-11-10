use crate::{Manager, error, publish_project};
use proj_model::ProjectInfo;
use zbus::Connection;
impl Manager {
    pub(super) async fn update_icon(
        &self,
        id: &str,
        glyph: String,
        automatic: bool,
        clear: bool,
        conn: &Connection,
    ) -> zbus::fdo::Result<ProjectInfo> {
        if !clear && (glyph.trim().is_empty() || glyph.len() > 64) {
            return Err(zbus::fdo::Error::InvalidArgs(
                "Icon must be a nonempty glyph, at most 64 bytes".into(),
            ));
        }
        let _lock = self.mutation.lock().await;
        let mut project = self
            .store
            .projects()
            .map_err(error)?
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| zbus::fdo::Error::FileNotFound("Project not found".into()))?;
        if automatic && !project.icon.is_empty() {
            return Ok(project);
        }
        project.icon = if clear { String::new() } else { glyph };
        project.icon_origin = if clear {
            ""
        } else if automatic {
            "automatic"
        } else {
            "manual"
        }
        .to_owned();
        self.store.put("project", id, &project).map_err(error)?;
        publish_project(conn, project.clone())
            .await
            .map_err(error)?;
        Ok(project)
    }
}
