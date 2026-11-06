use std::{
    sync::mpsc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use futures_util::StreamExt;
use graphyne::{GraphiteClient, GraphiteMessage};
use locus::RelationRecord;
use shell_rx_macros::combine_latest;
use shell_source::{
    Observable,
    dbus::{self, PropertyDescriptor},
    rx::Observable as _,
};
use zbus::zvariant::OwnedObjectPath;

use crate::{
    focus::{FocusState, GraphiteSlots, Tracker},
    relations::{self, LocusClient},
};

use crate::workspace::{object as workspace_object, root_property};

#[path = "metrics/agents.rs"]
mod agents;
#[path = "metrics/context.rs"]
mod context;
#[cfg(test)]
#[path = "metrics/probe.rs"]
mod probe;
#[cfg(test)]
#[path = "metrics/source_test.rs"]
mod source_test;

fn prefix() -> String {
    std::env::var("METRIC_PREFIX").unwrap_or_else(|_| "rsynapse".to_owned())
}

pub struct Metrics {
    locus: LocusClient,
    carbon_host: String,
    carbon_port: u16,
}

impl Metrics {
    pub fn new(locus: LocusClient, carbon_host: String, carbon_port: u16) -> Self {
        Self {
            locus,
            carbon_host,
            carbon_port,
        }
    }

    pub async fn run(self, mut shutdown: tokio::sync::watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut workdays = crate::workdays::Workdays::load(&self.locus).await?;
        let mut workday_targets = crate::workdays::target_preferences(&self.locus).into_stream();
        let mut productivity = crate::productivity::Tracker::load(&self.locus).await?;
        let mut policies = crate::productivity::policies(&self.locus).into_stream();
        // Bounded: a stalled Graphite host drops batches instead of growing memory.
        let (batch_sender, batch_receiver) = mpsc::sync_channel::<Vec<(String, f64, u64)>>(1024);
        let host = self.carbon_host.clone();
        let port = self.carbon_port;
        let carbon = std::thread::Builder::new()
            .name("carbon-push".to_owned())
            .spawn(move || run_carbon_push(batch_receiver, &host, port))
            .expect("spawn carbon push thread");

        let mut tracker = Tracker::new();
        let mut agents = crate::agent_metrics::AgentTracker::new();
        let mut usage_events = crate::agent_metrics::usage_events().into_stream();
        let mut agent_events = crate::agent_metrics::snapshots(&self.locus).into_stream();
        let mut goal_work = crate::goal_work::Recorder::connect().await?;
        let mut goal_updates = shell_source::proj::goal_tracking().into_stream();
        let mut current_focus = FocusState::default();
        let mut current_goals = Vec::new();
        let mut states = activity_focus(
            effective_focus(focus_state(&self.locus), shell_source::session::locked()),
            shell_source::wayland::input_idle(input_idle_threshold()?),
        )
        .into_stream();
        // Events are the only reporting trigger. No interval, heartbeat,
        // debounce, or timed flush exists in this collection path.
        let result = loop {
            tokio::select! {
                biased;
                _ = shutdown.changed() => break Ok(()),
                item = states.next() => match item {
                    Some(Ok(state)) => {
                        // Focus accounting/publishing must precede unrelated
                        // domain persistence, which can block or fail on D-Bus.
                        let observed = Instant::now();
                        tracker.observe(state.clone(), observed);
                        publish_events(&mut tracker, &batch_sender);
                        current_focus=state.clone();
                        goal_work.observe(crate::goal_work::scope(&state,&current_goals)).await;
                        productivity.focus(state.clone(), crate::productivity::tracker_now());
                        publish_pending(productivity.publish(&self.locus).await?, &batch_sender);
                        publish_pending(workdays.observe(&state, &self.locus).await?, &batch_sender);
                        agents.focus(state.window_id.clone(), !state.locked && !state.idle, observed);
                        publish_pending(agents.drain(), &batch_sender);
                    }
                    Some(Err(error)) => break Err(anyhow::anyhow!("focus/activity source failed: {error}")),
                    None => break Err(anyhow::anyhow!("focus/activity source ended")),
                },
                item=goal_updates.next()=>match item {
                    Some(Ok(goals))=>{
                        current_goals=goals;
                        goal_work.reconcile(crate::goal_work::scope(&current_focus,&current_goals)).await;
                    }
                    Some(Err(error))=>break Err(anyhow::anyhow!("goal tracking source failed: {error}")),
                    None=>break Err(anyhow::anyhow!("goal tracking source ended")),
                },
                item = usage_events.next() => match item {
                    Some(Ok(event)) => {
                        agents.usage(event);
                        publish_pending(agents.drain(), &batch_sender);
                    }
                    Some(Err(error)) => break Err(anyhow::anyhow!("agent usage source failed: {error}")),
                    None => break Err(anyhow::anyhow!("agent usage source ended")),
                },
                item = agent_events.next() => match item {
                    Some(Ok(snapshot)) => {
                        productivity.agents(snapshot.clone(), crate::productivity::tracker_now());
                        publish_pending(productivity.publish(&self.locus).await?, &batch_sender);
                        agents.update(snapshot, Instant::now());
                        publish_pending(agents.drain(), &batch_sender);
                    }
                    Some(Err(error)) => break Err(anyhow::anyhow!("agent source failed: {error}")),
                    None => break Err(anyhow::anyhow!("agent source ended")),
                },
                item = policies.next() => match item {
                    Some(Ok(policy)) => {
                        productivity.policy(policy, crate::productivity::tracker_now());
                        publish_pending(productivity.publish(&self.locus).await?, &batch_sender);
                    }
                    Some(Err(error)) => break Err(anyhow::anyhow!("productivity policy source failed: {error}")),
                    None => break Err(anyhow::anyhow!("productivity policy source ended")),
                },
                item = workday_targets.next() => match item {
                    Some(Ok(targets)) => publish_pending(workdays.set_targets(targets,&self.locus).await?,&batch_sender),
                    Some(Err(error)) => break Err(anyhow::anyhow!("workday target source failed: {error}")),
                    None => break Err(anyhow::anyhow!("workday target source ended")),
                }
            }
        };
        tracker.finish(Instant::now());
        goal_work.finish().await;
        agents.finish(Instant::now());
        productivity.finish(crate::productivity::tracker_now());
        publish_pending(productivity.publish(&self.locus).await?, &batch_sender);
        publish_pending(
            workdays
                .observe(
                    &FocusState {
                        locked: true,
                        ..FocusState::default()
                    },
                    &self.locus,
                )
                .await?,
            &batch_sender,
        );
        publish_events(&mut tracker, &batch_sender);
        publish_pending(agents.drain(), &batch_sender);
        drop(batch_sender);
        tokio::task::spawn_blocking(move || carbon.join())
            .await?
            .map_err(|_| anyhow::anyhow!("carbon push thread panicked"))?;
        result
    }
}

fn input_idle_threshold() -> anyhow::Result<Duration> {
    let seconds: u64 = std::env::var("INPUT_IDLE_SECS")
        .unwrap_or_else(|_| "30".to_owned())
        .parse()?;
    anyhow::ensure!(
        seconds > 0 && seconds <= u32::MAX as u64 / 1000,
        "INPUT_IDLE_SECS must be a positive Wayland timeout in seconds"
    );
    Ok(Duration::from_secs(seconds))
}

fn activity_focus(focus: Observable<FocusState>, idle: Observable<bool>) -> Observable<FocusState> {
    focus
        .combine_latest(idle, |state, idle| FocusState { idle, ..state })
        .distinct_until_changed()
        .box_it()
}

/// Lock changes independently terminate attribution, even without niri events.
/// Keep consuming niri while locked so unlocking resumes the latest focus.
fn effective_focus(
    focus: Observable<FocusState>,
    locked: Observable<bool>,
) -> Observable<FocusState> {
    focus
        .start_with(vec![FocusState::default()])
        .combine_latest(locked, |state, locked| {
            if locked {
                FocusState {
                    locked: true,
                    ..FocusState::default()
                }
            } else {
                state
            }
        })
        .distinct_until_changed()
        .box_it()
}

#[cfg(test)]
mod lock_tests {
    use super::*;
    use shell_source::rx::{ObservableFactory as _, Observer, Shared};

    #[tokio::test]
    async fn lock_without_niri_event_clears_focus_and_unlock_uses_latest() {
        let mut focus = Shared::subject::<FocusState, String>();
        let mut lock = Shared::subject::<bool, String>();
        let mut stream =
            effective_focus(focus.clone().box_it(), lock.clone().box_it()).into_stream();
        tokio::task::yield_now().await;
        lock.next(true);
        assert!(stream.next().await.unwrap().unwrap().locked);
        focus.next(FocusState {
            workspace_id: Some("10".into()),
            ..FocusState::default()
        });
        lock.next(false);
        assert_eq!(
            stream
                .next()
                .await
                .unwrap()
                .unwrap()
                .workspace_id
                .as_deref(),
            Some("10")
        );
        lock.next(true);
        let cleared = stream.next().await.unwrap().unwrap();
        assert!(cleared.locked);
        assert!(cleared.workspace_id.is_none());
        focus.next(FocusState {
            workspace_id: Some("11".into()),
            ..FocusState::default()
        });
        lock.next(false);
        let resumed = stream.next().await.unwrap().unwrap();
        assert!(!resumed.locked);
        assert_eq!(resumed.workspace_id.as_deref(), Some("11"));
    }

    #[tokio::test]
    async fn idle_retains_focus_and_lock_still_clears_it() {
        let mut focus = Shared::subject::<FocusState, String>();
        let mut lock = Shared::subject::<bool, String>();
        let mut idle = Shared::subject::<bool, String>();
        let mut stream = activity_focus(
            effective_focus(focus.clone().box_it(), lock.clone().box_it()),
            idle.clone().box_it(),
        )
        .into_stream();
        tokio::task::yield_now().await;
        lock.next(false);
        idle.next(false);
        stream.next().await;
        focus.next(FocusState {
            workspace_id: Some("10".into()),
            ..FocusState::default()
        });
        assert_eq!(
            stream
                .next()
                .await
                .unwrap()
                .unwrap()
                .workspace_id
                .as_deref(),
            Some("10")
        );
        idle.next(true);
        let paused = stream.next().await.unwrap().unwrap();
        assert!(paused.idle);
        assert_eq!(paused.workspace_id.as_deref(), Some("10"));
        lock.next(true);
        assert!(stream.next().await.unwrap().unwrap().locked);
        lock.next(false);
        let still_idle = stream.next().await.unwrap().unwrap();
        assert!(still_idle.idle);
        assert!(!still_idle.locked);
        assert_eq!(still_idle.workspace_id.as_deref(), Some("10"));
        focus.next(FocusState {
            workspace_id: Some("11".into()),
            ..FocusState::default()
        });
        let moved_while_idle = stream.next().await.unwrap().unwrap();
        assert!(moved_while_idle.idle);
        assert_eq!(moved_while_idle.workspace_id.as_deref(), Some("11"));
        idle.next(false);
        let resumed = stream.next().await.unwrap().unwrap();
        assert!(!resumed.idle);
        assert_eq!(resumed.workspace_id.as_deref(), Some("11"));
    }
}

/// Workspace/project context survives window switches and optional enrichment.
fn focus_state(locus: &LocusClient) -> Observable<FocusState> {
    combine_latest!(
        context::source(locus.clone()),
        window_focus(locus.clone())
        => move |(context, window): (context::Context, WindowFocus)| FocusState {
            locked: false,
            idle: false,
            workspace_id: context.workspace_id,
            workspace_name: context.workspace_name,
            window_id: window.window_id,
            app_id: window.app_id,
            project: context.project,
            project_id: context.project_id,
            output: window.output,
        },
    )
    .distinct_until_changed()
    .box_it()
}

#[derive(Clone, Debug, Default, PartialEq)]
struct WindowFocus {
    window_id: Option<String>,
    app_id: Option<String>,
    output: Option<String>,
}

fn window_focus(locus: LocusClient) -> Observable<WindowFocus> {
    let selection = combine_latest!(
        crate::workspace::focused_window_id(),
        focused_window(),
        focused_output()
        => |(window_id, window, output)| (window_id, window, output),
    )
    .distinct_until_changed()
    .box_it();
    shell_source::switch_map(selection, move |(window_id, window, output)| {
        let window = window.clone();
        // Rebuilt per switch on purpose: shared hubs dedupe by descriptor
        // key, so this reuses the live upstream instead of cloning streams.
        let instances = relations::records(locus.clone(), relations::WINDOW_APP_INSTANCE);
        let agents = relations::records(locus.clone(), relations::WINDOW_AGENT_SESSION);
        combine_latest!(
            resolve_app(window.clone(), window_id.clone(), instances, agents),
            resolve_output_name(output.clone())
            => move |(app, output_name): (Option<String>, Option<String>)| WindowFocus {
                window_id: window_id.clone(),
                app_id: app,
                output: output_name,
            },
        )
        .distinct_until_changed()
        .box_it()
    })
    .start_with(vec![WindowFocus::default()])
    .distinct_until_changed()
    .box_it()
}

fn focused_window() -> Observable<Option<OwnedObjectPath>> {
    dbus::optional_array_property::<OwnedObjectPath>(root_property("FocusedWindow"))
}

fn focused_output() -> Observable<Option<OwnedObjectPath>> {
    dbus::optional_array_property::<OwnedObjectPath>(root_property("FocusedOutput"))
}

/// Canonical app identity: live agent identity first (opencode, codex, …),
/// then hook app-instance name (neovim, …), then canonicalized niri AppId.
fn resolve_app(
    window: Option<OwnedObjectPath>,
    window_id: Option<String>,
    instances: Observable<Vec<RelationRecord>>,
    agents: Observable<Vec<RelationRecord>>,
) -> Observable<Option<String>> {
    let Some(path) = window else {
        return shell_source::once(None);
    };
    let hook = hook_app_name(instances, window_id.clone());
    let agent = agents::agent_app(agents, window_id);
    let app_id = dbus::optional_array_property::<String>(PropertyDescriptor::new(
        workspace_object(path.as_str(), workspace_dbus_api::WINDOW_INTERFACE),
        "AppId",
    ))
    .map(|app| app.map(|app| crate::focus::canonical_app_id(&app)));
    combine_latest!(
        agent,
        hook,
        app_id
        => move |(agent, hook, app_id)| agent.or(hook).or(app_id),
    )
    // Enrichment may arrive late; it must not freeze workspace/project timers.
    .start_with(vec![None])
    .distinct_until_changed()
    .box_it()
}

fn resolve_output_name(output: Option<OwnedObjectPath>) -> Observable<Option<String>> {
    let Some(path) = output else {
        return shell_source::once(None);
    };
    dbus::property_or(
        PropertyDescriptor::new(
            workspace_object(path.as_str(), workspace_dbus_api::OUTPUT_INTERFACE),
            "Name",
        ),
        String::new(),
    )
    .map(|name| (!name.is_empty()).then_some(name))
    .distinct_until_changed()
    .box_it()
}

fn hook_app_name(
    records: Observable<Vec<RelationRecord>>,
    window_id: Option<String>,
) -> Observable<Option<String>> {
    let Some(id) = window_id else {
        return shell_source::once(None);
    };
    let subject = locus::RelationEndpoint::stable_key(locus::keys::WINDOW_ID, id);
    records
        .map(move |records| {
            records.into_iter().find_map(|record| {
                if record.subject != subject {
                    return None;
                }
                record
                    .metadata
                    .get("app-name")
                    .cloned()
                    .filter(|name| !name.trim().is_empty())
            })
        })
        .distinct_until_changed()
        .box_it()
}

fn publish_events(tracker: &mut Tracker, batch_sender: &mpsc::SyncSender<Vec<(String, f64, u64)>>) {
    publish_pending(tracker.drain(), batch_sender);
}

fn publish_pending(
    pending: std::collections::HashMap<String, f64>,
    batch_sender: &mpsc::SyncSender<Vec<(String, f64, u64)>>,
) {
    if pending.is_empty() {
        return;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .unwrap_or(0);
    let batch = pending
        .into_iter()
        .map(|(metric, value)| (format!("{}.{metric}", prefix()), value, now))
        .collect::<Vec<_>>();
    match batch_sender.try_send(batch) {
        Ok(()) => {}
        Err(mpsc::TrySendError::Full(_)) => {
            eprintln!("[rsynapse-steward/metrics] carbon queue full; dropping batch");
        }
        Err(mpsc::TrySendError::Disconnected(_)) => {
            eprintln!("[rsynapse-steward/metrics] carbon push thread gone; dropping batch");
        }
    }
}

fn run_carbon_push(receiver: mpsc::Receiver<Vec<(String, f64, u64)>>, host: &str, port: u16) {
    let mut client = None::<GraphiteClient>;
    let mut slots = GraphiteSlots::default();
    while let Ok(batch) = receiver.recv() {
        let batch = slots.event(batch);
        if client.is_none() {
            match GraphiteClient::builder().address(host).port(port).build() {
                Ok(built) => client = Some(built),
                Err(error) => {
                    eprintln!("[rsynapse-steward/metrics] carbon failed: {error}; dropping batch");
                    continue;
                }
            }
        }
        let Some(active) = client.as_mut() else {
            continue;
        };
        let messages = batch
            .iter()
            .map(|(path, value, timestamp)| {
                GraphiteMessage::new_with_ts(path, &value.to_string(), *timestamp)
            })
            .collect::<Vec<_>>();
        if let Err(error) = active.send_batch_message(&messages) {
            eprintln!("[rsynapse-steward/metrics] carbon failed: {error}; dropping batch");
            client = None;
        }
    }
}
