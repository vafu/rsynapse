use std::{
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use futures_util::StreamExt;
use graphyne::{GraphiteClient, GraphiteMessage};
use locus::RelationRecord;
use shell_rx_macros::combine_latest;
use shell_source::{
    Observable,
    dbus::{self, Bus, ObjectDescriptor, PropertyDescriptor},
    rx::Observable as _,
};
use zbus::zvariant::OwnedObjectPath;

use crate::{
    focus::{FocusState, Tracker},
    relations::{self, LocusClient},
};

const WORKSPACE_PATH_PREFIX: &str = "/org/rsynapse/Niri/Workspaces/workspace_";
const WINDOW_PATH_PREFIX: &str = "/org/rsynapse/Niri/Windows/window_";

fn flush_secs() -> u64 {
    std::env::var("FLUSH_SECS")
        .ok()
        .and_then(|secs| secs.parse().ok())
        .filter(|secs| *secs > 0)
        .unwrap_or(10)
}

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

    pub async fn run(self) -> anyhow::Result<()> {
        let (batch_sender, batch_receiver) = mpsc::channel::<Vec<(String, f64, u64)>>();
        let host = self.carbon_host.clone();
        let port = self.carbon_port;
        std::thread::Builder::new()
            .name("carbon-push".to_owned())
            .spawn(move || run_carbon_push(batch_receiver, &host, port))
            .expect("spawn carbon push thread");

        let tracker = Arc::new(Mutex::new(Tracker::new(Instant::now())));
        let moved = tracker.clone();
        let mut states = focus_state(&self.locus).into_stream();
        let _pump = tokio::spawn(async move {
            while let Some(item) = states.next().await {
                match item {
                    Ok(state) => {
                        if let Ok(mut tracker) = moved.lock() {
                            tracker.observe(state, Instant::now());
                        }
                    }
                    Err(error) => eprintln!("[rsynapse-steward/metrics] focus failed: {error}"),
                }
            }
        });

        let mut flush = tokio::time::interval(Duration::from_secs(flush_secs()));
        flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            flush.tick().await;
            if let Ok(mut tracker) = tracker.lock() {
                tracker.heartbeat(Instant::now());
                flush_batch(&mut tracker, &batch_sender);
            }
        }
        #[allow(unreachable_code)]
        {
            _pump.abort();
            Ok(())
        }
    }
}

/// Full focus state: root focus paths combined, dynamic details resolved per
/// selection. Single process-wide subscriber; hubs stay connected while the
/// daemon runs, so workspace switches decode from signals with no re-reads.
fn focus_state(locus: &LocusClient) -> Observable<FocusState> {
    let selection = combine_latest!(
        focused_workspace(),
        focused_window(),
        focused_output()
        => |(workspace, window, output)| (workspace, window, output),
    )
    .distinct_until_changed()
    .box_it();
    let locus = locus.clone();
    shell_source::switch_map(selection, move |(workspace, window, output)| {
        let workspace = workspace.clone();
        let window = window.clone();
        // Rebuilt per switch on purpose: shared hubs dedupe by descriptor
        // key, so this reuses the live upstream instead of cloning streams.
        let projects = relations::records(locus.clone(), relations::WORKSPACE_PROJECT);
        let instances = relations::records(locus.clone(), relations::WINDOW_APP_INSTANCE);
        let names = relations::records(locus.clone(), relations::WORKSPACE_NAME);
        combine_latest!(
            resolve_app(window.clone(), instances),
            resolve_output_name(output.clone()),
            project_for(projects, workspace_id(&workspace)),
            explicit_name_for(names, workspace_id(&workspace))
            => move |(app, output_name, project, explicit_name)| FocusState {
                workspace_id: workspace_id(&workspace),
                // Explicit locus name wins; project display name covers
                // project-native workspaces; otherwise id-only.
                workspace_name: explicit_name.or(project.clone()),
                window_id: window_id(&window),
                app_id: app,
                project,
                output: output_name,
            },
        )
        .distinct_until_changed()
        .box_it()
    })
    .distinct_until_changed()
    .box_it()
}

fn focused_workspace() -> Observable<Option<OwnedObjectPath>> {
    dbus::optional_array_property::<OwnedObjectPath>(root_property("FocusedWorkspace"))
}

fn focused_window() -> Observable<Option<OwnedObjectPath>> {
    dbus::optional_array_property::<OwnedObjectPath>(root_property("FocusedWindow"))
}

fn focused_output() -> Observable<Option<OwnedObjectPath>> {
    dbus::optional_array_property::<OwnedObjectPath>(root_property("FocusedOutput"))
}

/// Canonical app identity: hook-written app-instance name first (codex,
/// neovim, …), else the canonicalized niri AppId. One identity per window.
fn resolve_app(
    window: Option<OwnedObjectPath>,
    instances: Observable<Vec<RelationRecord>>,
) -> Observable<Option<String>> {
    let Some(path) = window else {
        return shell_source::once(None);
    };
    let hook = hook_app_name(instances, window_id(&Some(path.clone())));
    let app_id = dbus::optional_array_property::<String>(PropertyDescriptor::new(
        niri_object(path.as_str(), niri_dbus::WINDOW_INTERFACE),
        "AppId",
    ))
    .map(|app| app.map(|app| crate::focus::canonical_app_id(&app)));
    combine_latest!(
        hook,
        app_id
        => move |(hook, app_id)| hook.or(app_id),
    )
    .distinct_until_changed()
    .box_it()
}

fn resolve_output_name(output: Option<OwnedObjectPath>) -> Observable<Option<String>> {
    let Some(path) = output else {
        return shell_source::once(None);
    };
    dbus::property_or(
        PropertyDescriptor::new(
            niri_object(path.as_str(), niri_dbus::OUTPUT_INTERFACE),
            "Name",
        ),
        String::new(),
    )
    .map(|name| (!name.is_empty()).then_some(name))
    .distinct_until_changed()
    .box_it()
}

fn project_for(
    records: Observable<Vec<RelationRecord>>,
    workspace_id: Option<u64>,
) -> Observable<Option<String>> {
    let Some(id) = workspace_id else {
        return shell_source::once(None);
    };
    let subject =
        locus::RelationEndpoint::stable_key(locus::keys::NIRI_WORKSPACE_ID, id.to_string());
    records
        .map(move |records| {
            records.into_iter().find_map(|record| {
                if record.subject != subject {
                    return None;
                }
                project_name(&record)
            })
        })
        .distinct_until_changed()
        .box_it()
}

fn hook_app_name(
    records: Observable<Vec<RelationRecord>>,
    window_id: Option<u64>,
) -> Observable<Option<String>> {    let Some(id) = window_id else {
        return shell_source::once(None);
    };
    let subject =
        locus::RelationEndpoint::stable_key(locus::keys::NIRI_WINDOW_ID, id.to_string());
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

/// Explicit workspace name from the namer-owned relation, if any.
fn explicit_name_for(
    records: Observable<Vec<RelationRecord>>,
    workspace_id: Option<u64>,
) -> Observable<Option<String>> {
    let Some(id) = workspace_id else {
        return shell_source::once(None);
    };
    let subject =
        locus::RelationEndpoint::stable_key(locus::keys::NIRI_WORKSPACE_ID, id.to_string());
    records
        .map(move |records| {
            records.into_iter().find_map(|record| {
                if record.subject != subject {
                    return None;
                }
                let name = match &record.target {
                    locus::RelationEndpoint::StableKey { kind, id }
                        if kind == relations::WORKSPACE_NAME_KIND =>
                    {
                        id.clone()
                    }
                    _ => return None,
                };
                (!name.trim().is_empty()).then_some(name)
            })
        })
        .distinct_until_changed()
        .box_it()
}

fn project_name(record: &RelationRecord) -> Option<String> {
    record
        .metadata
        .get("name")
        .cloned()
        .filter(|name| !name.trim().is_empty())
        .or_else(|| {
            record.metadata.get("path").and_then(|path| {
                std::path::Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_owned)
            })
        })
}

fn root_property(name: &'static str) -> PropertyDescriptor {
    PropertyDescriptor::new(
        niri_object(niri_dbus::ROOT_PATH, niri_dbus::ROOT_INTERFACE),
        name,
    )
}

fn niri_object(path: &str, interface: &str) -> ObjectDescriptor {
    ObjectDescriptor::parse(Bus::Session, niri_dbus::BUS_NAME, path, interface)
        .expect("niri descriptor should be valid")
}

fn workspace_id(path: &Option<OwnedObjectPath>) -> Option<u64> {
    path.as_ref()?
        .as_str()
        .strip_prefix(WORKSPACE_PATH_PREFIX)?
        .parse()
        .ok()
}

fn window_id(path: &Option<OwnedObjectPath>) -> Option<u64> {
    path.as_ref()?
        .as_str()
        .strip_prefix(WINDOW_PATH_PREFIX)?
        .parse()
        .ok()
}

fn flush_batch(
    tracker: &mut Tracker,
    batch_sender: &mpsc::Sender<Vec<(String, f64, u64)>>,
) {
    let pending = tracker.drain();
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
    if batch_sender.send(batch).is_err() {
        eprintln!("[rsynapse-steward/metrics] carbon push thread gone; dropping batch");
    }
}

fn run_carbon_push(receiver: mpsc::Receiver<Vec<(String, f64, u64)>>, host: &str, port: u16) {
    let mut client = None::<GraphiteClient>;
    while let Ok(batch) = receiver.recv() {
        if client.is_none() {
            match GraphiteClient::builder()
                .address(host)
                .port(port)
                .build()
            {
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
