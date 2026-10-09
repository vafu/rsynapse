//! Grafana HTTP/SSE client adapter. Projd owns all project-management records.
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderValue, Method, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
    routing::{get, put},
};
use futures_util::{StreamExt, stream};
use goal_model::{
    AGENT_GOAL, GOAL_KIND, Goal, GoalInfo, GoalType, PREFERENCES_KIND, Status, WORKDAY_TARGETS,
    WorkdayTargets,
};
use locus::{RelationEndpoint, RelationRecord, RelationState};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    convert::Infallible,
    sync::Arc,
};
use tokio::sync::{Mutex, broadcast};
use tower_http::cors::CorsLayer;
use zbus::{Connection, Proxy};

#[derive(Clone)]
struct App {
    proj: Proxy<'static>,
    locus: Proxy<'static>,
    conn: Connection,
    writes: Arc<Mutex<()>>,
    events: broadcast::Sender<()>,
}
#[derive(Debug)]
struct Error(StatusCode, String);
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({"error":self.1}))).into_response()
    }
}
impl From<zbus::Error> for Error {
    fn from(e: zbus::Error) -> Self {
        let status = match &e {
            zbus::Error::MethodError(n, _, _) if n.as_str().ends_with("FileExists") => {
                StatusCode::CONFLICT
            }
            zbus::Error::MethodError(n, _, _) if n.as_str().ends_with("FileNotFound") => {
                StatusCode::NOT_FOUND
            }
            zbus::Error::MethodError(n, _, _) if n.as_str().ends_with("InvalidArgs") => {
                StatusCode::BAD_REQUEST
            }
            _ => StatusCode::SERVICE_UNAVAILABLE,
        };
        Self(status, e.to_string())
    }
}
fn invalid(e: impl Into<String>) -> Error {
    Error(StatusCode::BAD_REQUEST, e.into())
}
type Result<T> = std::result::Result<T, Error>;
fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}
impl App {
    async fn associations(&self) -> Result<Proxy<'_>> {
        Ok(Proxy::new(
            &self.conn,
            "org.rsynapse.Steward",
            "/org/rsynapse/Steward",
            "org.rsynapse.Steward.Associations1",
        )
        .await?)
    }
    async fn records(&self, relation: &str) -> Result<Vec<RelationRecord>> {
        Ok(self.locus.call("List", &(relation,)).await?)
    }
    async fn goals(&self, date: &str) -> Result<Vec<Goal>> {
        let rows: Vec<GoalInfo> = self.proj.call("ListGoals", &(date,)).await?;
        rows.into_iter()
            .map(|g| Goal::try_from(g).map_err(invalid))
            .collect()
    }
    async fn find(&self, date: &str, id: &str) -> Result<Goal> {
        self.goals(date)
            .await?
            .into_iter()
            .find(|g| g.id == id)
            .ok_or_else(|| Error(StatusCode::NOT_FOUND, "Goal not found".into()))
    }
    async fn write(&self, goal: &Goal) -> Result<Goal> {
        let info: GoalInfo = self
            .proj
            .call("UpdateGoal", &(GoalInfo::from(goal.clone()),))
            .await?;
        Goal::try_from(info).map_err(invalid)
    }
}
#[derive(Deserialize)]
struct DayQuery {
    date: Option<String>,
}
async fn goals(State(app): State<App>, Query(query): Query<DayQuery>) -> Result<Json<Vec<Goal>>> {
    let day = query.date.unwrap_or_else(today);
    if !goal_model::valid_date(&day) {
        return Err(invalid("Date must be YYYY-MM-DD"));
    }
    Ok(Json(app.goals(&day).await?))
}
async fn days(State(app): State<App>) -> Result<Json<serde_json::Value>> {
    let today = today();
    let mut days: BTreeSet<_> = app.goals("").await?.into_iter().map(|g| g.date).collect();
    days.insert(today.clone());
    Ok(Json(serde_json::json!({"today":today,"days":days})))
}
async fn create(
    State(app): State<App>,
    Json(goal): Json<Goal>,
) -> Result<(StatusCode, Json<Goal>)> {
    goal.validate().map_err(invalid)?;
    let _lock = app.writes.lock().await;
    let info: GoalInfo = app
        .proj
        .call("CreateGoal", &(GoalInfo::from(goal),))
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(Goal::try_from(info).map_err(invalid)?),
    ))
}
async fn update(
    State(app): State<App>,
    Path((date, id)): Path<(String, String)>,
    Json(goal): Json<Goal>,
) -> Result<Json<Goal>> {
    if goal.date != date || goal.id != id {
        return Err(invalid("Date and ID cannot change during editing"));
    }
    goal.validate().map_err(invalid)?;
    let _lock = app.writes.lock().await;
    Ok(Json(app.write(&goal).await?))
}
async fn edit(
    State(app): State<App>,
    Path((date, id)): Path<(String, String)>,
    Json(patch): Json<serde_json::Map<String, serde_json::Value>>,
) -> Result<Json<Goal>> {
    let _lock = app.writes.lock().await;
    let original = app.find(&date, &id).await?;
    let mut value = serde_json::to_value(&original).map_err(|e| invalid(e.to_string()))?;
    for (key, value2) in patch {
        if !matches!(
            key.as_str(),
            "title" | "kind" | "project" | "success" | "priority" | "status"
        ) {
            return Err(invalid(format!("Field {key} cannot be edited")));
        }
        value[&key] = value2;
    }
    let goal: Goal = serde_json::from_value(value).map_err(|e| invalid(e.to_string()))?;
    goal.validate().map_err(invalid)?;
    if original.kind == GoalType::Outcome && goal.kind == GoalType::Habit {
        let _: () = app
            .associations()
            .await?
            .call("ClearGoalSessions", &(goal.key(),))
            .await?;
    }
    Ok(Json(app.write(&goal).await?))
}
#[derive(Deserialize)]
struct StatusUpdate {
    status: Status,
}
async fn status(
    State(app): State<App>,
    Path((date, id)): Path<(String, String)>,
    Json(update): Json<StatusUpdate>,
) -> Result<Json<Goal>> {
    let _lock = app.writes.lock().await;
    let mut goal = app.find(&date, &id).await?;
    goal.status = update.status;
    Ok(Json(app.write(&goal).await?))
}
async fn remove(
    State(app): State<App>,
    Path((date, id)): Path<(String, String)>,
) -> Result<StatusCode> {
    let _lock = app.writes.lock().await;
    let goal = app.find(&date, &id).await?;
    let _: () = app
        .associations()
        .await?
        .call("ClearGoalSessions", &(goal.key(),))
        .await?;
    let _: bool = app.proj.call("RemoveGoal", &(date, id)).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn target_settings(State(app): State<App>) -> Result<Json<WorkdayTargets>> {
    let targets = app
        .records(WORKDAY_TARGETS)
        .await?
        .into_iter()
        .find_map(|r| {
            let RelationEndpoint::StableKey { kind, id } = r.subject else {
                return None;
            };
            if kind != PREFERENCES_KIND || id != "default" {
                return None;
            }
            serde_json::from_str::<WorkdayTargets>(r.metadata.get("targets")?)
                .ok()
                .filter(|t| t.validate().is_ok())
        })
        .unwrap_or_default();
    Ok(Json(targets))
}
async fn save_targets(
    State(app): State<App>,
    Json(targets): Json<WorkdayTargets>,
) -> Result<Json<WorkdayTargets>> {
    targets.validate().map_err(invalid)?;
    let _lock = app.writes.lock().await;
    let _: RelationState = app
        .locus
        .call(
            "SetOneWithPersistence",
            &(
                RelationEndpoint::stable_key(PREFERENCES_KIND, "default"),
                WORKDAY_TARGETS,
                RelationEndpoint::stable_key(WORKDAY_TARGETS, "default"),
                HashMap::from([(
                    "targets".to_owned(),
                    serde_json::to_string(&targets).map_err(|e| invalid(e.to_string()))?,
                )]),
                true,
            ),
        )
        .await?;
    Ok(Json(targets))
}
#[derive(Clone, Serialize, Deserialize)]
struct Link {
    agent: String,
    session_id: String,
}
fn session_key(link: &Link) -> Result<String> {
    if !goal_model::valid_id(&link.agent) || link.session_id.trim().is_empty() {
        return Err(invalid("Agent and session ID required"));
    }
    let agent = match link.agent.as_str() {
        "opencode-cli" => "opencode",
        "codex-cli" => "codex",
        s => s,
    };
    Ok(format!("{agent}/{}", link.session_id))
}
async fn links(
    State(app): State<App>,
    Path((date, id)): Path<(String, String)>,
) -> Result<Json<Vec<Link>>> {
    let goal = app.find(&date, &id).await?;
    let target = RelationEndpoint::stable_key(GOAL_KIND, goal.key());
    Ok(Json(
        app.records(AGENT_GOAL)
            .await?
            .into_iter()
            .filter_map(|r| {
                if r.target != target {
                    return None;
                }
                let RelationEndpoint::StableKey { kind, id } = r.subject else {
                    return None;
                };
                if kind != locus::keys::AGENT_SESSION_ID {
                    return None;
                }
                let (agent, session_id) = id.split_once('/')?;
                Some(Link {
                    agent: agent.into(),
                    session_id: session_id.into(),
                })
            })
            .collect(),
    ))
}
async fn link(
    State(app): State<App>,
    Path((date, id)): Path<(String, String)>,
    Json(link): Json<Link>,
) -> Result<StatusCode> {
    session_key(&link)?;
    let _lock = app.writes.lock().await;
    let goal = app.find(&date, &id).await?;
    if goal.kind != GoalType::Outcome {
        return Err(invalid("Habits cannot grant agent-work credit"));
    }
    let _: () = app
        .associations()
        .await?
        .call("BindAgentGoal", &(goal.key(), link.agent, link.session_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn unlink(
    State(app): State<App>,
    Path((date, id)): Path<(String, String)>,
    Json(link): Json<Link>,
) -> Result<StatusCode> {
    session_key(&link)?;
    let _lock = app.writes.lock().await;
    let goal = app.find(&date, &id).await?;
    let _: () = app
        .associations()
        .await?
        .call(
            "UnbindAgentGoal",
            &(goal.key(), link.agent, link.session_id),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn projects(State(app): State<App>) -> Result<Json<BTreeSet<String>>> {
    let rows: Vec<goal_model::ProjectInfo> = app.proj.call("ListProjects", &()).await?;
    Ok(Json(rows.into_iter().map(|p| p.cwd).collect()))
}
async fn sessions(State(app): State<App>) -> Result<Json<Vec<serde_json::Value>>> {
    use zbus::zvariant::{OwnedObjectPath, OwnedValue};
    type Objects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;
    let proxy = Proxy::new(
        &app.conn,
        "io.github.AgentDBus",
        "/io/github/AgentDBus",
        "org.freedesktop.DBus.ObjectManager",
    )
    .await?;
    let objects: Objects = match proxy.call("GetManagedObjects", &()).await {
        Ok(o) => o,
        Err(_) => return Ok(Json(vec![])),
    };
    let mut rows = vec![];
    for i in objects.values() {
        if let Some(p) = i.get("io.github.AgentDBus1.Session") {
            let text = |n: &str| {
                p.get(n)
                    .and_then(|v| <&str>::try_from(v).ok())
                    .unwrap_or("")
                    .to_owned()
            };
            if p.get("IsSubagent")
                .and_then(|v| bool::try_from(v).ok())
                .unwrap_or(false)
            {
                continue;
            }
            rows.push(serde_json::json!({"agent":text("AgentName"),"session_id":text("SessionId"),"title":text("SessionTitle"),"state":text("State"),"cwd":text("Cwd")}));
        }
    }
    Ok(Json(rows))
}
async fn events(
    State(app): State<App>,
) -> Sse<impl futures_util::Stream<Item = std::result::Result<Event, Infallible>>> {
    let rx = app.events.subscribe();
    Sse::new(stream::unfold((rx, true), |(mut rx, first)| async move {
        if first {
            return Some((
                Ok(Event::default().event("connected").data("ready")),
                (rx, false),
            ));
        }
        match rx.recv().await {
            Ok(()) | Err(broadcast::error::RecvError::Lagged(_)) => Some((
                Ok(Event::default().event("goals-changed").data("refresh")),
                (rx, false),
            )),
            Err(broadcast::error::RecvError::Closed) => None,
        }
    }))
}
async fn watch(app: App) -> anyhow::Result<()> {
    let mut domain = app.proj.receive_signal("Changed").await?;
    let mut added = app.locus.receive_signal("RelationAdded").await?;
    let mut updated = app.locus.receive_signal("RelationUpdated").await?;
    let mut removed = app.locus.receive_signal("RelationRemoved").await?;
    let mut cleared = app.locus.receive_signal("RelationCleared").await?;
    tokio::spawn(async move {
        loop {
            let message = tokio::select! {m=domain.next()=>{if m.is_some(){let _=app.events.send(());continue;}else{None}},m=added.next()=>m,m=updated.next()=>m,m=removed.next()=>m,m=cleared.next()=>m};
            let Some(m) = message else {
                break;
            };
            let relevant = m
                .body()
                .deserialize::<RelationRecord>()
                .is_ok_and(|r| r.relation == AGENT_GOAL || r.relation == WORKDAY_TARGETS)
                || m.body()
                    .deserialize::<(RelationEndpoint, String, u32)>()
                    .is_ok_and(|(_, r, _)| r == AGENT_GOAL || r == WORKDAY_TARGETS);
            if relevant {
                let _ = app.events.send(());
            }
        }
    });
    Ok(())
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let conn = Connection::session().await?;
    let locus = Proxy::new_owned(
        conn.clone(),
        locus::BUS_NAME,
        locus::OBJECT_PATH,
        locus::RELATIONS_INTERFACE,
    )
    .await?;
    let proj = Proxy::new_owned(
        conn.clone(),
        goal_model::BUS_NAME,
        goal_model::ROOT_PATH,
        goal_model::MANAGER_INTERFACE,
    )
    .await?;
    let (event_sender, _) = broadcast::channel(32);
    let app = App {
        proj,
        locus,
        conn,
        writes: Arc::new(Mutex::new(())),
        events: event_sender,
    };
    watch(app.clone()).await?;
    let origins = std::env::var("RSYNAPSE_DASHBOARD_ORIGINS")
        .unwrap_or_else(|_| "http://localhost:3000,http://127.0.0.1:3000".into());
    let origins: Vec<HeaderValue> = origins
        .split(',')
        .map(str::trim)
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()?;
    let cors = CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([axum::http::header::CONTENT_TYPE]);
    let router = Router::new()
        .route("/api/goals", get(goals).post(create))
        .route("/api/goal-days", get(days))
        .route(
            "/api/goals/{date}/{id}",
            put(update).patch(edit).delete(remove),
        )
        .route(
            "/api/goals/{date}/{id}/status",
            axum::routing::patch(status),
        )
        .route(
            "/api/goals/{date}/{id}/links",
            get(links).post(link).delete(unlink),
        )
        .route(
            "/api/workday-targets",
            get(target_settings).put(save_targets),
        )
        .route("/api/projects", get(projects))
        .route("/api/sessions", get(sessions))
        .route("/api/events", get(events))
        .route(
            "/health",
            get(|| async { Json(serde_json::json!({"ok":true})) }),
        )
        .layer(axum::extract::DefaultBodyLimit::max(65536))
        .layer(cors)
        .with_state(app);
    let port = std::env::var("RSYNAPSE_DASHBOARD_PORT")
        .unwrap_or_else(|_| "8770".into())
        .parse::<u16>()?;
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    eprintln!("dashboard adapter listening on http://127.0.0.1:{port}");
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
