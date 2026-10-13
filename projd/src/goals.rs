use crate::{Manager, Store, error, notes, publish_goal, publish_project};
use proj_model::{Goal, GoalInfo, ProjectInfo, goal_path};
use zbus::Connection;

pub(super) fn project_id(store: &Store, path: &str) -> anyhow::Result<String> {
    let checkouts = store.checkouts()?;
    Ok(store
        .projects()?
        .into_iter()
        .filter(|p| {
            let root = checkouts
                .iter()
                .find(|c| c.id == p.checkout_id)
                .map(|c| c.root_path.as_str())
                .unwrap_or(&p.cwd);
            !path.is_empty() && std::path::Path::new(path).starts_with(root)
        })
        .max_by_key(|p| {
            checkouts
                .iter()
                .find(|c| c.id == p.checkout_id)
                .map(|c| c.root_path.len())
                .unwrap_or(p.cwd.len())
        })
        .map(|p| p.id)
        .unwrap_or_default())
}

pub(super) fn initialize(store: &Store) -> anyhow::Result<()> {
    for mut goal in store.goals()? {
        let before = goal.clone();
        if goal.project_id.is_empty() {
            goal.project_id = project_id(store, &goal.project)?;
        }
        let _: Goal = goal.clone().try_into().map_err(anyhow::Error::msg)?;
        notes::ensure(&mut goal)?;
        if goal != before {
            store.put("goal", &goal.key(), &goal)?;
        }
    }
    Ok(())
}

impl Manager {
    pub(super) async fn write_goal(
        &self,
        mut info: GoalInfo,
        create: bool,
        conn: &Connection,
    ) -> zbus::fdo::Result<GoalInfo> {
        let _: Goal = info
            .clone()
            .try_into()
            .map_err(zbus::fdo::Error::InvalidArgs)?;
        let _lock = self.mutation.lock().await;
        let goals = self.store.goals().map_err(error)?;
        let key = info.key();
        let existing = goals.iter().find(|g| g.key() == key);
        if create && existing.is_some() {
            return Err(zbus::fdo::Error::FileExists("Goal already exists".into()));
        }
        if !create && existing.is_none() {
            return Err(zbus::fdo::Error::FileNotFound("Goal not found".into()));
        }
        if let Some(old) = existing {
            if info.parent_id != old.parent_id {
                return Err(zbus::fdo::Error::InvalidArgs(
                    "Parent cannot change during editing".into(),
                ));
            }
            if info.project != old.project {
                if !old.parent_id.is_empty() || goals.iter().any(|g| g.parent_id == key) {
                    return Err(zbus::fdo::Error::InvalidArgs(
                        "Cannot change project for a task hierarchy".into(),
                    ));
                }
                self.clear_active(&key, conn).await?;
                info.project_id = project_id(&self.store, &info.project).map_err(error)?;
            } else {
                info.project_id = old.project_id.clone();
            }
            // Note contents and identity are independent of task metadata edits.
            info.note_path = old.note_path.clone();
        } else {
            info.project_id = project_id(&self.store, &info.project).map_err(error)?;
            info.note_path.clear();
            if !info.parent_id.is_empty() {
                let parent = goals
                    .iter()
                    .find(|g| g.key() == info.parent_id)
                    .ok_or_else(|| {
                        zbus::fdo::Error::FileNotFound("Parent goal not found".into())
                    })?;
                if parent.project != info.project || parent.project_id != info.project_id {
                    return Err(zbus::fdo::Error::InvalidArgs(
                        "Subtasks must belong to their parent's project".into(),
                    ));
                }
                let mut ancestor = parent;
                let mut depth = 1;
                while !ancestor.parent_id.is_empty() {
                    depth += 1;
                    if depth > 64 {
                        return Err(zbus::fdo::Error::InvalidArgs(
                            "Task hierarchy is too deep".into(),
                        ));
                    }
                    ancestor = goals
                        .iter()
                        .find(|g| g.key() == ancestor.parent_id)
                        .ok_or_else(|| error("Task hierarchy has a missing parent"))?;
                }
            }
        }
        info = tokio::task::spawn_blocking(move || {
            notes::ensure(&mut info)?;
            Ok::<_, anyhow::Error>(info)
        })
        .await
        .map_err(error)?
        .map_err(error)?;
        if create {
            if !self.store.create("goal", &key, &info).map_err(error)? {
                return Err(zbus::fdo::Error::FileExists("Goal already exists".into()));
            }
        } else {
            self.store.put("goal", &key, &info).map_err(error)?;
        }
        publish_goal(conn, info.clone()).await.map_err(error)?;
        if create && !info.parent_id.is_empty() {
            if let Some(parent) = goals.iter().find(|g| g.key() == info.parent_id) {
                self.children_changed(conn, parent).await?;
            }
        }
        if matches!(info.status.as_str(), "completed" | "deferred") {
            self.clear_active(&key, conn).await?;
        }
        Ok(info)
    }

    async fn children_changed(&self, conn: &Connection, goal: &GoalInfo) -> zbus::fdo::Result<()> {
        if let Ok(handle) = conn
            .object_server()
            .interface::<_, crate::GoalObject>(goal.object_path())
            .await
        {
            handle
                .get()
                .await
                .children_changed(handle.signal_context())
                .await
                .map_err(error)?;
        }
        Ok(())
    }

    pub(super) async fn clear_active(&self, key: &str, conn: &Connection) -> zbus::fdo::Result<()> {
        for project in self.store.projects().map_err(error)? {
            if self.store.active_goal(&project.id).map_err(error)? == key {
                self.store
                    .remove("active-goal", &project.id)
                    .map_err(error)?;
                self.active_changed(conn, &project).await?;
            }
        }
        Ok(())
    }

    pub(super) async fn active_changed(
        &self,
        conn: &Connection,
        project: &ProjectInfo,
    ) -> zbus::fdo::Result<()> {
        publish_project(conn, project.clone())
            .await
            .map_err(error)?;
        let handle = conn
            .object_server()
            .interface::<_, crate::ProjectObject>(proj_model::object_path("Projects", &project.id))
            .await
            .map_err(error)?;
        handle
            .get()
            .await
            .active_goal_changed(handle.signal_context())
            .await
            .map_err(error)?;
        Ok(())
    }

    pub(super) async fn delete_goal(
        &self,
        date: &str,
        id: &str,
        conn: &Connection,
    ) -> zbus::fdo::Result<bool> {
        let _lock = self.mutation.lock().await;
        let key = proj_model::goal_key(date, id);
        let goals = self.store.goals().map_err(error)?;
        let Some(goal) = goals.iter().find(|g| g.key() == key) else {
            return Ok(false);
        };
        if goals.iter().any(|g| g.parent_id == key) {
            return Err(zbus::fdo::Error::InvalidArgs(
                "Remove subtasks before removing their parent".into(),
            ));
        }
        self.store.remove("goal", &key).map_err(error)?;
        self.clear_active(&key, conn).await?;
        conn.object_server()
            .remove::<crate::GoalObject, _>(goal_path(date, id))
            .await
            .map_err(error)?;
        if let Some(parent) = goals.iter().find(|g| g.key() == goal.parent_id) {
            self.children_changed(conn, parent).await?;
        }
        // Retain the Markdown note as user/agent-authored history.
        Ok(true)
    }
}
