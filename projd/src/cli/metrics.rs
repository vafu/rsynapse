use chrono::{DateTime, Local, NaiveDate, TimeZone};
use clap::{Args, Subcommand};
use proj_model::*;
use std::{collections::BTreeMap, path::PathBuf};
use zbus::Proxy;

#[derive(Args)]
pub struct Range {
    /// Local date (midnight), RFC3339 timestamp, or Unix milliseconds.
    #[arg(long)]
    from: Option<String>,
    /// Exclusive range end; defaults to now.
    #[arg(long)]
    to: Option<String>,
    #[arg(long)]
    json: bool,
}
impl Range {
    fn bounds(&self) -> anyhow::Result<(u64, u64)> {
        let from = self
            .from
            .as_deref()
            .map(timestamp)
            .transpose()?
            .unwrap_or(0);
        let to = self.to.as_deref().map(timestamp).transpose()?.unwrap_or(0);
        anyhow::ensure!(to == 0 || to >= from, "Range end precedes start");
        Ok((from, to))
    }
}
#[derive(Args)]
pub struct ReportArgs {
    #[arg(long,conflicts_with_all=["project_id","all"])]
    project: Option<PathBuf>,
    #[arg(long, conflicts_with = "all")]
    project_id: Option<String>,
    #[arg(long)]
    all: bool,
    #[command(flatten)]
    range: Range,
}
#[derive(Subcommand)]
pub enum MetricsCommand {
    /// List immutable task identities, including deleted/reused keys.
    Instances {
        id: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Inspect creation/status/selection/edit/removal history.
    History {
        id: String,
        #[command(flatten)]
        range: Range,
    },
    /// List measured focus intervals; includes descendant work unless --direct.
    Timeline {
        id: Option<String>,
        #[arg(long)]
        direct: bool,
        #[command(flatten)]
        report: ReportArgs,
    },
    /// Show effort and lifecycle timing separately. Zero timestamps mean unknown.
    Stats {
        id: String,
        #[arg(long)]
        direct: bool,
        #[command(flatten)]
        range: Range,
    },
}
fn timestamp(input: &str) -> anyhow::Result<u64> {
    if let Ok(ms) = input.parse::<u64>() {
        anyhow::ensure!(ms <= i64::MAX as u64, "Timestamp is out of range");
        return Ok(ms);
    }
    if let Ok(date) = DateTime::parse_from_rfc3339(input) {
        return u64::try_from(date.timestamp_millis()).map_err(Into::into);
    }
    let date = if input == "today" {
        Local::now().date_naive()
    } else {
        NaiveDate::parse_from_str(input, "%Y-%m-%d")?
    };
    let at = Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
        .earliest()
        .ok_or_else(|| anyhow::anyhow!("Date has no local midnight"))?;
    Ok(u64::try_from(at.timestamp_millis())?)
}
async fn metrics<'a>(proxy: &'a Proxy<'_>) -> zbus::Result<Proxy<'a>> {
    Proxy::new(proxy.connection(), BUS_NAME, ROOT_PATH, METRICS_INTERFACE).await
}
async fn identities(proxy: &Proxy<'_>, query: &str) -> anyhow::Result<Vec<GoalIdentityInfo>> {
    Ok(proxy.call("ListGoalIdentities", &(query,)).await?)
}
async fn identity(proxy: &Proxy<'_>, query: &str) -> anyhow::Result<GoalIdentityInfo> {
    let rows = identities(proxy, query).await?;
    if let Some(row) = rows.iter().find(|g| g.instance_id == query) {
        return Ok(row.clone());
    }
    let keys: std::collections::BTreeSet<_> = rows.iter().map(|g| g.key.clone()).collect();
    anyhow::ensure!(
        keys.len() == 1,
        "Goal not found or ambiguous; use its date/ID key or instance UUID"
    );
    if let Some(row) = rows.iter().find(|g| g.deleted_at_ms == 0) {
        return Ok(row.clone());
    }
    rows.into_iter()
        .max_by_key(|g| g.observed_at_ms)
        .ok_or_else(|| anyhow::anyhow!("Goal not found"))
}
async fn project_id(manager: &Proxy<'_>, args: &ReportArgs) -> anyhow::Result<String> {
    if args.all {
        return Ok(String::new());
    }
    if let Some(id) = &args.project_id {
        return Ok(id.clone());
    }
    if let Ok(project) = super::goal_cli::project(manager, args.project.clone()).await {
        return Ok(project.id);
    }
    let path = super::absolute(args.project.clone().unwrap_or_else(|| PathBuf::from(".")))?;
    let proxy = metrics(manager).await?;
    let ids: std::collections::BTreeSet<_> = identities(&proxy, "")
        .await?
        .iter()
        .filter(|g| g.goal.project == path)
        .map(|g| g.goal.project_id.clone())
        .filter(|id| !id.is_empty())
        .collect();
    anyhow::ensure!(
        ids.len() == 1,
        "Project is unknown or has multiple historical identities; use --project-id or --all"
    );
    Ok(ids.into_iter().next().unwrap())
}
async fn timeline(
    manager: &Proxy<'_>,
    args: &ReportArgs,
    id: Option<&str>,
    direct: bool,
) -> anyhow::Result<Vec<WorkSessionInfo>> {
    let proxy = metrics(manager).await?;
    let (from, to) = args.range.bounds()?;
    let goal = if let Some(id) = id {
        identity(&proxy, id).await?.instance_id
    } else {
        String::new()
    };
    let project = if id.is_some() && args.project.is_none() && args.project_id.is_none() {
        String::new()
    } else {
        project_id(manager, args).await?
    };
    Ok(proxy
        .call("WorkTimeline", &(goal, project, from, to, !direct))
        .await?)
}
fn when(ms: u64) -> String {
    if ms == 0 {
        return "unknown".into();
    }
    DateTime::from_timestamp_millis(ms as i64)
        .map(|d| d.with_timezone(&Local).to_rfc3339())
        .unwrap_or_else(|| "unknown".into())
}
fn duration(ms: u64) -> String {
    format!(
        "{}h {:02}m {:02}s",
        ms / 3_600_000,
        (ms / 60_000) % 60,
        (ms / 1000) % 60
    )
}

pub async fn run(manager: &Proxy<'_>, command: MetricsCommand) -> anyhow::Result<()> {
    let proxy = metrics(manager).await?;
    match command {
        MetricsCommand::Instances { id, json } => {
            let rows = identities(&proxy, id.as_deref().unwrap_or("")).await?;
            if json {
                println!("{}", serde_json::to_string(&rows)?);
            } else {
                for g in rows {
                    println!(
                        "{} {} {} [{}]",
                        g.instance_id,
                        g.key,
                        g.goal.title,
                        if g.deleted_at_ms > 0 {
                            "deleted"
                        } else {
                            "current"
                        }
                    );
                }
            }
        }
        MetricsCommand::History { id, range } => {
            let goal = identity(&proxy, &id).await?;
            let (from, to) = range.bounds()?;
            let rows: Vec<GoalEventInfo> = proxy
                .call("GoalHistory", &(goal.instance_id, from, to))
                .await?;
            if range.json {
                println!("{}", serde_json::to_string(&rows)?);
            } else {
                for e in rows {
                    println!(
                        "{} {:16} {} [{} → {}]",
                        when(e.at_ms),
                        e.kind,
                        e.goal.title,
                        e.old_status,
                        e.goal.status
                    );
                }
            }
        }
        MetricsCommand::Timeline { id, direct, report } => {
            let rows = timeline(manager, &report, id.as_deref(), direct).await?;
            if report.range.json {
                println!("{}", serde_json::to_string(&rows)?);
            } else {
                for s in rows {
                    println!(
                        "{} — {}  {:?} {}  {} / {}{}",
                        when(s.started_at_ms),
                        when(s.ended_at_ms),
                        s.kind,
                        duration(s.duration_ms),
                        s.project_name,
                        s.goal_title,
                        if s.open { " [open]" } else { "" }
                    );
                }
            }
        }
        MetricsCommand::Stats { id, direct, range } => {
            let goal = identity(&proxy, &id).await?;
            let (from, to) = range.bounds()?;
            let stats: GoalStatsInfo = proxy
                .call("GoalStats", &(goal.instance_id, from, to, !direct))
                .await?;
            if range.json {
                println!("{}", serde_json::to_string(&stats)?);
            } else {
                println!(
                    "{} [{}]\nProject: {}\nInput-active focus: {}\nIdle focus: {}\nUnlocked focus: {}\nAgent work: {}\nIn-progress elapsed: {}\nCreated: {}\nFirst observed: {}\nFirst started: {}\nLast completed: {}\nCompletion lead time: {}\nStart-to-completion: {}\nCompletions: {}\nHistory partial: {}\nInterrupted sessions: {}",
                    stats.title,
                    stats.status,
                    stats.project_name,
                    duration(stats.active_ms),
                    duration(stats.idle_ms),
                    duration(stats.active_ms + stats.idle_ms),
                    duration(stats.agent_ms),
                    duration(stats.in_progress_ms),
                    when(stats.created_at_ms),
                    when(stats.observed_at_ms),
                    when(stats.first_started_at_ms),
                    when(stats.completed_at_ms),
                    if stats.created_at_ms == 0 || stats.completed_at_ms == 0 {
                        "unknown".into()
                    } else {
                        duration(stats.lead_time_ms)
                    },
                    if stats.first_started_at_ms == 0 || stats.completed_at_ms == 0 {
                        "unknown".into()
                    } else {
                        duration(stats.cycle_time_ms)
                    },
                    stats.completion_count,
                    stats.history_partial,
                    stats.interrupted_sessions
                );
            }
        }
    }
    Ok(())
}

pub async fn report(manager: &Proxy<'_>, args: ReportArgs) -> anyhow::Result<()> {
    let rows = timeline(manager, &args, None, false).await?;
    let mut totals: BTreeMap<(String, String), serde_json::Value> = BTreeMap::new();
    for s in rows {
        let row=totals.entry((s.project_id.clone(),s.instance_id.clone())).or_insert_with(||serde_json::json!({"project_id":s.project_id,"project":s.project_name,"instance_id":s.instance_id,"goal_key":s.goal_key,"goal":s.goal_title,"active_ms":0u64,"idle_ms":0u64,"agent_ms":0u64,"sessions":0u64,"interrupted_sessions":0u64}));
        let field = match s.kind {
            WorkKind::Active => "active_ms",
            WorkKind::Idle => "idle_ms",
            WorkKind::Agent => "agent_ms",
        };
        row[field] = serde_json::json!(row[field].as_u64().unwrap() + s.duration_ms);
        row["sessions"] = serde_json::json!(row["sessions"].as_u64().unwrap() + 1);
        if s.end_reason.contains("unknown-duration") {
            row["interrupted_sessions"] =
                serde_json::json!(row["interrupted_sessions"].as_u64().unwrap() + 1);
        }
    }
    let rows: Vec<_> = totals.into_values().collect();
    if args.range.json {
        println!("{}", serde_json::to_string(&rows)?);
    } else {
        for r in rows {
            println!(
                "{} / {}  active={} idle={} agent={}",
                r["project"].as_str().unwrap(),
                r["goal"].as_str().unwrap(),
                duration(r["active_ms"].as_u64().unwrap()),
                duration(r["idle_ms"].as_u64().unwrap()),
                duration(r["agent_ms"].as_u64().unwrap())
            );
        }
    }
    Ok(())
}
