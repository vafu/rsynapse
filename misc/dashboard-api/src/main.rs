//! Grafana HTTP/SSE client adapter. Projd owns all project-management records.
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderValue, Method, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
    routing::get,
};
use futures_util::{StreamExt, stream};
use goal_model::{
    PREFERENCES_KIND, WORKDAY_TARGETS, WorkdayTargets,
    api::{ManagerProxy, MetricsProxy},
};
use locus::{RelationEndpoint, RelationRecord, RelationState, RelationsProxy};
use std::{collections::HashMap, convert::Infallible, sync::Arc};
use tokio::sync::{Mutex, broadcast};
use tower_http::cors::CorsLayer;
use zbus::Connection;
mod metrics;

#[derive(Clone, Copy)]
enum Change {
    Goals,
    Metrics,
    Targets,
}

#[derive(Clone)]
struct App {
    proj: ManagerProxy<'static>,
    metrics: MetricsProxy<'static>,
    locus: RelationsProxy<'static>,
    writes: Arc<Mutex<()>>,
    events: broadcast::Sender<Change>,
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
impl App {
    async fn records(&self, relation: &str) -> Result<Vec<RelationRecord>> {
        Ok(self.locus.list(relation).await?)
    }
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
        .set_one_with_persistence(
            RelationEndpoint::stable_key(PREFERENCES_KIND, "default"),
            WORKDAY_TARGETS,
            RelationEndpoint::stable_key(WORKDAY_TARGETS, "default"),
            HashMap::from([(
                "targets".to_owned(),
                serde_json::to_string(&targets).map_err(|e| invalid(e.to_string()))?,
            )]),
            true,
        )
        .await?;
    Ok(Json(targets))
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
            Ok(change) => Some((
                Ok(Event::default()
                    .event(match change {
                        Change::Goals => "goals-changed",
                        Change::Metrics => "goal-metrics-changed",
                        Change::Targets => "workday-targets-changed",
                    })
                    .data("refresh")),
                (rx, false),
            )),
            Err(broadcast::error::RecvError::Lagged(_)) => Some((
                Ok(Event::default().event("connected").data("refresh")),
                (rx, false),
            )),
            Err(broadcast::error::RecvError::Closed) => None,
        }
    }))
}
async fn watch(app: App) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let mut domain = app.proj.receive_changed().await?;
    let mut work = app.metrics.receive_work_changed().await?;
    let mut added = app.locus.receive_relation_added().await?;
    let mut updated = app.locus.receive_relation_updated().await?;
    let mut removed = app.locus.receive_relation_removed().await?;
    let mut cleared = app.locus.receive_relation_cleared().await?;
    Ok(tokio::spawn(async move {
        loop {
            tokio::select! {
                message = domain.next() => {
                    let Some(message) = message else {
                        eprintln!("[dashboard] projd change stream ended; restart to reconnect");
                        break;
                    };
                    if message.args().is_ok_and(|args| args.kind != "checkout") {
                        let _ = app.events.send(Change::Goals);
                    }
                }
                message = work.next() => {
                    if message.is_some() {
                        let _ = app.events.send(Change::Metrics);
                    } else {
                        eprintln!("[dashboard] projd work stream ended; restart to reconnect");
                        break;
                    }
                }
                message = added.next() => {
                    let Some(message) = message else {
                        eprintln!("[dashboard] locus RelationAdded stream ended");
                        break;
                    };
                    if message
                        .args()
                        .is_ok_and(|args| args.record.relation == WORKDAY_TARGETS)
                    {
                        let _ = app.events.send(Change::Targets);
                    }
                }
                message = updated.next() => {
                    let Some(message) = message else {
                        eprintln!("[dashboard] locus RelationUpdated stream ended");
                        break;
                    };
                    if message
                        .args()
                        .is_ok_and(|args| args.record.relation == WORKDAY_TARGETS)
                    {
                        let _ = app.events.send(Change::Targets);
                    }
                }
                message = removed.next() => {
                    let Some(message) = message else {
                        eprintln!("[dashboard] locus RelationRemoved stream ended");
                        break;
                    };
                    if message
                        .args()
                        .is_ok_and(|args| args.record.relation == WORKDAY_TARGETS)
                    {
                        let _ = app.events.send(Change::Targets);
                    }
                }
                message = cleared.next() => {
                    let Some(message) = message else {
                        eprintln!("[dashboard] locus RelationCleared stream ended");
                        break;
                    };
                    if message
                        .args()
                        .is_ok_and(|args| args.relation == WORKDAY_TARGETS)
                    {
                        let _ = app.events.send(Change::Targets);
                    }
                }
            }
        }
    }))
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let conn = Connection::session().await?;
    let locus = RelationsProxy::new(&conn).await?;
    let proj = ManagerProxy::new(&conn).await?;
    let (event_sender, _) = broadcast::channel(32);
    let metrics = MetricsProxy::new(&conn).await?;
    let app = App {
        proj,
        metrics,
        locus,
        writes: Arc::new(Mutex::new(())),
        events: event_sender,
    };
    let _watch = watch(app.clone()).await?;
    let origins = std::env::var("RSYNAPSE_DASHBOARD_ORIGINS")
        .unwrap_or_else(|_| "http://localhost:3000,http://127.0.0.1:3000".into());
    let origins: Vec<HeaderValue> = origins
        .split(',')
        .map(str::trim)
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()?;
    let cors = CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::PUT])
        .allow_headers([axum::http::header::CONTENT_TYPE]);
    let router = Router::new()
        .route("/api/goal-metrics", get(metrics::query))
        .route(
            "/api/workday-targets",
            get(target_settings).put(save_targets),
        )
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
