mod report;
use crate::{Store, error};
use futures_util::StreamExt;
use proj_model::*;
use std::{collections::HashMap, sync::Arc, time::Instant};
use tokio::sync::Mutex;
use zbus::{Connection, interface, message::Header, object_server::SignalContext};

pub fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}
#[derive(Clone)]
pub struct Metrics {
    store: Arc<Store>,
    clocks: Arc<Mutex<HashMap<String, Instant>>>,
    actions: Arc<Mutex<()>>,
    mutation: Arc<Mutex<()>>,
}
impl Metrics {
    pub fn new(store: Arc<Store>, mutation: Arc<Mutex<()>>) -> Self {
        Self {
            store,
            clocks: Arc::new(Mutex::new(HashMap::new())),
            actions: Arc::new(Mutex::new(())),
            mutation,
        }
    }
    async fn finish(
        &self,
        id: &str,
        owner: Option<&str>,
        reason: &str,
    ) -> zbus::fdo::Result<WorkSessionInfo> {
        let mut clocks = self.clocks.lock().await;
        let (actual_owner, mut session) = self
            .store
            .work_session(id)
            .map_err(error)?
            .ok_or_else(|| zbus::fdo::Error::FileNotFound("Work session not found".into()))?;
        if owner.is_some_and(|o| o != actual_owner) {
            return Err(zbus::fdo::Error::AccessDenied(
                "Work session belongs to another producer".into(),
            ));
        }
        if !session.open {
            return Ok(session);
        }
        session.duration_ms = clocks
            .remove(id)
            .map(|t| t.elapsed().as_millis().min(u64::MAX as u128) as u64)
            .unwrap_or(0);
        session.open = false;
        session.ended_at_ms = now_ms().max(session.started_at_ms);
        session.end_reason = reason.into();
        self.store
            .save_work(&actual_owner, &session)
            .map_err(error)?;
        Ok(session)
    }
    pub async fn close_all(&self, reason: &str) -> zbus::fdo::Result<()> {
        let _action = self.actions.lock().await;
        for (_, s) in self.store.open_work("").map_err(error)? {
            self.finish(&s.id, None, reason).await?;
        }
        Ok(())
    }
    pub async fn monitor_owners(&self, conn: Connection) -> anyhow::Result<()> {
        let metrics = self.clone();
        let (ready, wait) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let result = async {
                let bus = zbus::fdo::DBusProxy::new(&conn).await?;
                let mut events = bus.receive_name_owner_changed().await?;
                let _ = ready.send(());
                while let Some(event) = events.next().await {
                    let args = event.args()?;
                    if args.new_owner().is_some() {
                        continue;
                    }
                    let owner = args.name().as_str();
                    let _action = metrics.actions.lock().await;
                    for (producer, session) in metrics.store.open_work(owner)? {
                        if session.open && producer == owner {
                            metrics
                                .finish(&session.id, None, "source-disconnected")
                                .await?;
                            let signal = SignalContext::new(&conn, ROOT_PATH)?;
                            Self::work_changed(&signal, &session.id).await?;
                        }
                    }
                }
                Ok::<_, anyhow::Error>(())
            }
            .await;
            if let Err(e) = result {
                eprintln!("[projd/work-owner] {e:#}");
            }
        });
        wait.await?;
        Ok(())
    }
    async fn timeline(
        &self,
        goal: &str,
        project: &str,
        from: u64,
        to: u64,
        descendants: bool,
    ) -> zbus::fdo::Result<Vec<WorkSessionInfo>> {
        let end = if to == 0 { now_ms() } else { to };
        if end < from || end > i64::MAX as u64 {
            return Err(zbus::fdo::Error::InvalidArgs("Invalid time range".into()));
        }
        let clocks = self.clocks.lock().await;
        let mut result = Vec::new();
        for mut session in self
            .store
            .query_work(goal, project, from, end, descendants)
            .map_err(error)?
        {
            if session.open {
                session.ended_at_ms = now_ms().max(session.started_at_ms);
                session.duration_ms = clocks
                    .get(&session.id)
                    .map(|t| t.elapsed().as_millis().min(u64::MAX as u128) as u64)
                    .unwrap_or(0);
            }
            if let Some(session) = report::clip(session, from, end) {
                result.push(session);
            }
        }
        Ok(result)
    }
}
fn owner(header: &Header<'_>) -> zbus::fdo::Result<String> {
    header
        .sender()
        .map(|s| s.as_str().to_owned())
        .ok_or_else(|| zbus::fdo::Error::InvalidArgs("Missing producer identity".into()))
}

#[interface(name = "org.rsynapse.Proj.Metrics1")]
impl Metrics {
    async fn get_work_session(&self, session_id: String) -> zbus::fdo::Result<WorkSessionInfo> {
        let (_, mut session) = self
            .store
            .work_session(&session_id)
            .map_err(error)?
            .ok_or_else(|| zbus::fdo::Error::FileNotFound("Work session not found".into()))?;
        if session.open {
            session.duration_ms = self
                .clocks
                .lock()
                .await
                .get(&session_id)
                .map(|t| t.elapsed().as_millis().min(u64::MAX as u128) as u64)
                .unwrap_or(0);
        }
        Ok(session)
    }
    async fn list_goal_identities(
        &self,
        query: String,
    ) -> zbus::fdo::Result<Vec<GoalIdentityInfo>> {
        Ok(self
            .store
            .identities()
            .map_err(error)?
            .into_iter()
            .filter(|g| {
                query.is_empty() || g.instance_id == query || g.key == query || g.goal.id == query
            })
            .collect())
    }
    async fn list_goal_tracking(&self) -> zbus::fdo::Result<Vec<GoalTrackingInfo>> {
        self.store.tracking().map_err(error)
    }
    async fn goal_history(
        &self,
        instance_id: String,
        from_ms: u64,
        to_ms: u64,
    ) -> zbus::fdo::Result<Vec<GoalEventInfo>> {
        let end = if to_ms == 0 {
            now_ms().saturating_add(1)
        } else {
            to_ms
        };
        if end < from_ms || end > i64::MAX as u64 {
            return Err(zbus::fdo::Error::InvalidArgs("Invalid time range".into()));
        }
        self.store
            .history(&instance_id, from_ms, end)
            .map_err(error)
    }
    async fn work_timeline(
        &self,
        instance_id: String,
        project_id: String,
        from_ms: u64,
        to_ms: u64,
        include_subgoals: bool,
    ) -> zbus::fdo::Result<Vec<WorkSessionInfo>> {
        self.timeline(&instance_id, &project_id, from_ms, to_ms, include_subgoals)
            .await
    }
    async fn goal_stats(
        &self,
        instance_id: String,
        from_ms: u64,
        to_ms: u64,
        include_subgoals: bool,
    ) -> zbus::fdo::Result<GoalStatsInfo> {
        let mut identity = self
            .store
            .identities()
            .map_err(error)?
            .into_iter()
            .find(|g| g.instance_id == instance_id)
            .ok_or_else(|| zbus::fdo::Error::FileNotFound("Goal instance not found".into()))?;
        let end = if to_ms == 0 { now_ms() } else { to_ms };
        if let Some(project) = self
            .store
            .projects()
            .map_err(error)?
            .iter()
            .find(|p| p.id == identity.goal.project_id)
        {
            identity.project_name = project.name.clone();
        }
        let events = self
            .store
            .history(&instance_id, 0, now_ms().saturating_add(1))
            .map_err(error)?;
        let sessions = self
            .timeline(&instance_id, "", from_ms, end, include_subgoals)
            .await?;
        Ok(report::stats(identity, &events, &sessions, from_ms, end))
    }
    async fn begin_goal_work(
        &self,
        instance_id: String,
        kind: WorkKind,
        source: String,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_context)] signal: SignalContext<'_>,
    ) -> zbus::fdo::Result<WorkSessionInfo> {
        let _domain = self.mutation.lock().await;
        let _action = self.actions.lock().await;
        if source.is_empty() || source.len() > 128 {
            return Err(zbus::fdo::Error::InvalidArgs(
                "Source must be 1–128 bytes".into(),
            ));
        }
        let producer = owner(&header)?;
        let identity = self
            .store
            .identities()
            .map_err(error)?
            .into_iter()
            .find(|g| {
                g.instance_id == instance_id
                    && g.deleted_at_ms == 0
                    && !matches!(g.goal.status.as_str(), "completed" | "deferred")
            })
            .ok_or_else(|| zbus::fdo::Error::FileNotFound("Open goal instance not found".into()))?;
        for (owner, s) in self.store.open_work(&producer).map_err(error)? {
            if owner == producer && s.open && s.source == source {
                if s.instance_id == instance_id
                    && s.kind == kind
                    && s.project_id == identity.goal.project_id
                {
                    return Ok(s);
                }
                self.finish(&s.id, Some(&producer), "scope-replaced")
                    .await?;
            }
        }
        let project_name = self
            .store
            .projects()
            .map_err(error)?
            .iter()
            .find(|p| p.id == identity.goal.project_id)
            .map(|p| p.name.clone())
            .unwrap_or(identity.project_name);
        let session = WorkSessionInfo {
            id: uuid::Uuid::new_v4().simple().to_string(),
            instance_id,
            goal_key: identity.key,
            goal_title: identity.goal.title,
            project_id: identity.goal.project_id,
            project_name,
            ancestor_ids: identity.ancestor_ids,
            source,
            kind,
            started_at_ms: now_ms(),
            ended_at_ms: 0,
            duration_ms: 0,
            open: true,
            end_reason: String::new(),
        };
        let mut clocks = self.clocks.lock().await;
        self.store.save_work(&producer, &session).map_err(error)?;
        clocks.insert(session.id.clone(), Instant::now());
        drop(clocks);
        Self::work_changed(&signal, &session.id)
            .await
            .map_err(error)?;
        Ok(session)
    }
    async fn end_goal_work(
        &self,
        session_id: String,
        reason: String,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_context)] signal: SignalContext<'_>,
    ) -> zbus::fdo::Result<WorkSessionInfo> {
        let _action = self.actions.lock().await;
        if reason.len() > 128 {
            return Err(zbus::fdo::Error::InvalidArgs("Reason is too long".into()));
        }
        let session = self
            .finish(&session_id, Some(&owner(&header)?), &reason)
            .await?;
        Self::work_changed(&signal, &session.id)
            .await
            .map_err(error)?;
        Ok(session)
    }
    #[zbus(signal)]
    async fn work_changed(signal: &SignalContext<'_>, session_id: &str) -> zbus::Result<()>;
}
