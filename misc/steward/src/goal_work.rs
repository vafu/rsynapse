use crate::focus::FocusState;
use goal_model::{GoalTrackingInfo, WorkKind, api::MetricsProxy};

#[derive(Clone, Debug, PartialEq)]
pub struct Scope {
    pub instance_id: String,
    pub project_id: String,
    pub kind: WorkKind,
}

pub fn scope(state: &FocusState, goals: &[GoalTrackingInfo]) -> Option<Scope> {
    if state.locked {
        return None;
    }
    let project = state.project_id.as_ref()?;
    let mut selected = goals.iter().find(|g| {
        g.project_id == *project
            && g.selected
            && matches!(g.status.as_str(), "planned" | "in-progress")
    })?;
    let mut visited = vec![selected.instance_id.as_str()];
    loop {
        let mut children = goals.iter().filter(|g| {
            g.project_id == *project
                && g.parent_instance_id == selected.instance_id
                && g.status == "in-progress"
        });
        let Some(child) = children.next() else {
            break;
        };
        if children.next().is_some() || visited.contains(&child.instance_id.as_str()) {
            break;
        }
        visited.push(child.instance_id.as_str());
        selected = child;
    }
    Some(Scope {
        instance_id: selected.instance_id.clone(),
        project_id: project.clone(),
        kind: if state.idle {
            WorkKind::Idle
        } else {
            WorkKind::Active
        },
    })
}

pub struct Recorder {
    proxy: MetricsProxy<'static>,
    current: Option<(Scope, String)>,
}
impl Recorder {
    pub async fn connect() -> zbus::Result<Self> {
        let conn = zbus::Connection::session().await?;
        Ok(Self {
            proxy: MetricsProxy::new(&conn).await?,
            current: None,
        })
    }
    pub async fn observe(&mut self, wanted: Option<Scope>) {
        if self.current.as_ref().map(|(s, _)| s) == wanted.as_ref() {
            return;
        }
        if let Some((_, id)) = self.current.take() {
            if let Err(error) = self.proxy.end_goal_work(&id, "context-changed").await {
                eprintln!("[steward/goal-work] close: {error}");
            }
        }
        if let Some(scope) = wanted {
            match self
                .proxy
                .begin_goal_work(&scope.instance_id, scope.kind, "steward.focus")
                .await
            {
                Ok(session) => self.current = Some((scope, session.id)),
                Err(error) => eprintln!("[steward/goal-work] begin: {error}"),
            }
        }
    }
    pub async fn finish(&mut self) {
        if let Some((_, id)) = self.current.take() {
            if let Err(error) = self.proxy.end_goal_work(&id, "source-stopped").await {
                eprintln!("[steward/goal-work] shutdown: {error}");
            }
        }
    }
    pub async fn reconcile(&mut self, wanted: Option<Scope>) {
        // Reconnect notifications carry a fresh tracking snapshot. An interrupted
        // old session must not suppress a new observation after recorder restart.
        if let Some((_, id)) = &self.current {
            let session = self.proxy.get_work_session(id).await;
            if session.as_ref().is_ok_and(|s| !s.open) {
                self.current = None;
            }
        }
        self.observe(wanted).await;
    }
}

#[cfg(test)]
mod test;
