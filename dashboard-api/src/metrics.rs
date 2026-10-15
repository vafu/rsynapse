//! Read-only Grafana projection of projd's authoritative event/work history.
use crate::{App, Error, Result, invalid};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use futures_util::{StreamExt, TryStreamExt, stream};
use goal_model::{GoalEventInfo, GoalIdentityInfo, GoalStatsInfo, WorkKind, WorkSessionInfo};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Deserialize)]
pub(super) struct MetricsQuery {
    #[serde(default)]
    from_ms: u64,
    #[serde(default)]
    to_ms: u64,
    #[serde(default)]
    project_id: String,
    #[serde(default)]
    instance_id: String,
    #[serde(default = "yes")]
    include_subgoals: bool,
}
fn yes() -> bool {
    true
}
#[derive(Serialize)]
pub(super) struct MetricsView {
    from_ms: u64,
    to_ms: u64,
    queried_at_ms: u64,
    projects: Vec<ProjectChoice>,
    identities: Vec<GoalIdentityInfo>,
    stats: Vec<GoalStatsInfo>,
    work: Vec<WorkSessionInfo>,
    history: Vec<GoalEventInfo>,
}
#[derive(Serialize)]
struct ProjectChoice {
    id: String,
    name: String,
}

pub(super) async fn query(
    State(app): State<App>,
    Query(mut q): Query<MetricsQuery>,
) -> Result<Json<MetricsView>> {
    let now = chrono::Utc::now().timestamp_millis().max(0) as u64;
    if q.to_ms == 0 {
        q.to_ms = now;
    }
    if q.to_ms < q.from_ms || q.to_ms > i64::MAX as u64 {
        return Err(invalid("Invalid millisecond time range"));
    }
    let identities: Vec<GoalIdentityInfo> = app
        .metrics
        .call("ListGoalIdentities", &(String::new(),))
        .await?;
    if !q.instance_id.is_empty() && !identities.iter().any(|g| g.instance_id == q.instance_id) {
        return Err(Error(
            StatusCode::NOT_FOUND,
            "Goal instance not found".into(),
        ));
    }
    let all_work: Vec<WorkSessionInfo> = app
        .metrics
        .call(
            "WorkTimeline",
            &(String::new(), String::new(), q.from_ms, q.to_ms, true),
        )
        .await?;
    let mut projects = BTreeMap::new();
    for g in &identities {
        if !g.goal.project_id.is_empty() {
            projects.insert(g.goal.project_id.clone(), g.project_name.clone());
        }
    }
    for w in &all_work {
        projects.insert(w.project_id.clone(), w.project_name.clone());
    }
    let work: Vec<_> = all_work
        .into_iter()
        .filter(|w| {
            (q.project_id.is_empty() || w.project_id == q.project_id)
                && (q.instance_id.is_empty()
                    || w.instance_id == q.instance_id
                    || (q.include_subgoals && w.ancestor_ids.contains(&q.instance_id)))
        })
        .collect();
    let scope: HashSet<_> = identities
        .iter()
        .filter(|g| {
            (q.instance_id.is_empty()
                || g.instance_id == q.instance_id
                || (q.include_subgoals && g.ancestor_ids.contains(&q.instance_id)))
                && (q.project_id.is_empty()
                    || g.goal.project_id == q.project_id
                    || work.iter().any(|w| w.instance_id == g.instance_id))
        })
        .map(|g| g.instance_id.clone())
        .collect();
    let mut history: Vec<GoalEventInfo> = app
        .metrics
        .call("GoalHistory", &(String::new(), q.from_ms, q.to_ms))
        .await?;
    history.retain(|e| {
        scope.contains(&e.instance_id)
            && (q.project_id.is_empty() || e.goal.project_id == q.project_id)
    });
    let ids: Vec<String> = identities
        .iter()
        .filter(|g| scope.contains(&g.instance_id))
        .map(|g| g.instance_id.clone())
        .collect();
    let mut stats: Vec<GoalStatsInfo> = stream::iter(ids.into_iter().map(|id| {
        let metrics = app.metrics.clone();
        let from = q.from_ms;
        let to = q.to_ms;
        let descendants = q.include_subgoals;
        async move {
            metrics
                .call::<_, _, GoalStatsInfo>("GoalStats", &(id, from, to, descendants))
                .await
        }
    }))
    .buffer_unordered(8)
    .try_collect::<Vec<_>>()
    .await?;
    // Reuse the filtered interval snapshot for all effort fields: it preserves
    // project scoping after task moves and prevents open-span query-time skew.
    for s in &mut stats {
        if !q.project_id.is_empty() {
            s.project_id = q.project_id.clone();
            s.project_name = projects.get(&q.project_id).cloned().unwrap_or_default();
        }
        s.active_ms = 0;
        s.idle_ms = 0;
        s.agent_ms = 0;
        s.interrupted_sessions = 0;
        for w in work.iter().filter(|w| {
            w.instance_id == s.instance_id
                || (q.include_subgoals && w.ancestor_ids.contains(&s.instance_id))
        }) {
            match w.kind {
                WorkKind::Active => s.active_ms += w.duration_ms,
                WorkKind::Idle => s.idle_ms += w.duration_ms,
                WorkKind::Agent => s.agent_ms += w.duration_ms,
            }
            if w.end_reason.contains("unknown-duration") {
                s.interrupted_sessions += 1;
            }
        }
    }
    stats.sort_by(|a, b| {
        (b.active_ms + b.idle_ms + b.agent_ms)
            .cmp(&(a.active_ms + a.idle_ms + a.agent_ms))
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.instance_id.cmp(&b.instance_id))
    });
    Ok(Json(MetricsView {
        from_ms: q.from_ms,
        to_ms: q.to_ms,
        queried_at_ms: now,
        projects: projects
            .into_iter()
            .map(|(id, name)| ProjectChoice { id, name })
            .collect(),
        identities,
        stats,
        work,
        history,
    }))
}
