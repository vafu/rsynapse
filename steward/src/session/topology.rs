//! Preserve logical groups across output removal. One operation gate protects
//! capture and regrouping; cleanup calls this before refreshing window anchors.
use super::{
    Manager, links,
    model::{self, Session},
    observe,
};
use locus::{RelationEndpoint, RelationRecord};
use std::collections::{BTreeMap, BTreeSet};

pub(super) const CHECKPOINT: &str = "before-output-change";

#[derive(Clone)]
pub(super) struct Baseline {
    pub source: observe::Snapshot,
    pub session: Session,
    /// Persistent logical identity -> current native projection identity.
    pub slots: BTreeMap<String, String>,
    pub windows: BTreeMap<String, String>,
    pub recovery_focus: Option<Option<String>>,
}

fn outputs(source: &observe::Snapshot) -> BTreeSet<&str> {
    source.outputs.iter().map(String::as_str).collect()
}

fn lost_outputs<'a>(base: &'a Baseline, current: &observe::Snapshot) -> BTreeSet<&'a str> {
    let present = outputs(current);
    outputs(&base.source)
        .into_iter()
        .filter(|output| !present.contains(output))
        .collect()
}

pub(super) fn needs_reconciliation(
    base: &Baseline,
    current: &observe::Snapshot,
    records: &[RelationRecord],
) -> bool {
    base.source.owner == current.owner
        && base.source.generation == current.generation
        && (!lost_outputs(base, current).is_empty()
            || base.session.workspaces.iter().any(|w| {
                links::preferred_output(records, &w.id)
                    .is_some_and(|(out, _, fallback)| fallback && current.outputs.contains(&out))
            }))
}

fn plan(
    base: &Baseline,
    current: &observe::Snapshot,
    records: &[RelationRecord],
) -> anyhow::Result<Session> {
    let lost = lost_outputs(base, current);
    let mut recovery = Session {
        version: 1,
        name: CHECKPOINT.into(),
        workspaces: Vec::new(),
    };
    for saved in &base.session.workspaces {
        let original = base.slots.get(&saved.id);
        let old_output = original
            .and_then(|id| base.source.workspaces.get(id))
            .map(|w| w.output.as_str())
            .unwrap_or(&saved.output);
        let preference = links::preferred_output(records, &saved.id);
        let returning = preference
            .as_ref()
            .is_some_and(|(output, _, fallback)| *fallback && current.outputs.contains(output));
        if !lost.contains(old_output) && !returning {
            continue;
        }
        let mut workspace = saved.clone();
        workspace.windows.clear();
        let mut homes = BTreeSet::new();
        let mut destinations = BTreeSet::new();
        for window in &saved.windows {
            let Some(native) = base.windows.get(&window.id) else {
                continue;
            };
            let Some(live) = current.windows.get(native) else {
                continue;
            }; // closed normally
            let home = current
                .workspaces
                .get(&live.workspace)
                .ok_or_else(|| anyhow::anyhow!("waiting for evacuated window membership"))?;
            homes.insert(live.workspace.clone());
            destinations.insert(home.output.clone());
            let mut window = window.clone();
            window.recipe.clear(); // this path can only reuse existing windows
            window.floating = live.floating;
            workspace.windows.push(window);
        }
        if workspace.windows.is_empty() {
            continue;
        }
        if homes.len() == 1 {
            let home = homes.first().unwrap();
            let already_grouped = original.is_some_and(|id| home == id)
                || links::target(records, links::LIVE_SLOT, locus::keys::WORKSPACE_ID, home)
                    .is_some_and(|record| {
                        links::current(record, current)
                            && links::key(&record.target, links::SLOT).as_deref()
                                == Some(saved.id.as_str())
                    });
            let preferred_here = !returning
                || preference
                    .as_ref()
                    .is_some_and(|(output, _, _)| current.workspaces[home].output == *output);
            if already_grouped && preferred_here {
                continue;
            }
        }
        anyhow::ensure!(
            destinations.len() == 1,
            "evacuated group spans multiple outputs; keeping checkpoint for explicit recovery"
        );
        if returning {
            let (output, index, _) = preference.unwrap();
            workspace.output = output;
            workspace.index = index;
        } else {
            workspace.output = destinations.into_iter().next().unwrap();
        }
        recovery.workspaces.push(workspace);
    }
    recovery.workspaces.sort_by(|a, b| {
        let key = |w: &model::Workspace| {
            links::preferred_output(records, &w.id)
                .map(|(output, index, _)| (output, index))
                .unwrap_or_else(|| (w.output.clone(), w.index))
        };
        key(a).cmp(&key(b))
    });
    Ok(recovery)
}

impl Manager {
    async fn bootstrap_topology(&self) -> anyhow::Result<Option<Baseline>> {
        let records = self.locus.list("").await?;
        if !records.iter().any(|r| {
            r.relation == links::OUTPUT_PREFERENCE
                && r.metadata.get("fallback").is_some_and(|v| v == "true")
        }) {
            return Ok(None);
        }
        let Ok(session) = self.store.lock().await.get(CHECKPOINT) else {
            return Ok(None);
        };
        let current = observe::read(&self.connection).await?;
        let mut windows = BTreeMap::new();
        let mut slots = BTreeMap::new();
        for record in records.iter().filter(|r| links::current(r, &current)) {
            if record.relation == links::LIVE_WINDOW {
                if let (Some(saved), Some(native)) = (
                    links::key(&record.target, links::SAVED_WINDOW),
                    links::key(&record.subject, locus::keys::WINDOW_ID),
                ) {
                    if current.windows.contains_key(&native) {
                        windows.insert(saved, native);
                    }
                }
            } else if record.relation == links::LIVE_SLOT {
                if let (Some(saved), Some(native)) = (
                    links::key(&record.target, links::SLOT),
                    links::key(&record.subject, locus::keys::WORKSPACE_ID),
                ) {
                    if current.workspaces.contains_key(&native) {
                        slots.insert(saved, native);
                    }
                }
            }
        }
        if !session
            .workspaces
            .iter()
            .flat_map(|w| &w.windows)
            .any(|w| windows.contains_key(&w.id))
        {
            return Ok(None);
        }
        let mut source = current;
        source.outputs = session
            .workspaces
            .iter()
            .map(|w| w.output.clone())
            .collect();
        Ok(Some(Baseline {
            source,
            session,
            slots,
            windows,
            recovery_focus: None,
        }))
    }
    /// Cleanup must call outside BIND_LOCK: regrouping uses the normal explicit
    /// association operations, which acquire that lock themselves.
    pub(crate) async fn handle_topology(&self) -> anyhow::Result<()> {
        let _guard = self.operations.lock().await;
        self.reconcile_topology().await
    }

    /// Called with the session operation gate held, before capture can mutate
    /// logical links/checkpoints or old cleanup can overwrite window anchors.
    pub(super) async fn reconcile_topology(&self) -> anyhow::Result<()> {
        if self.baseline.lock().await.is_none() {
            if let Some(base) = self.bootstrap_topology().await? {
                *self.baseline.lock().await = Some(base);
            }
        }
        let Some(base) = self.baseline.lock().await.clone() else {
            return Ok(());
        };
        let current = observe::read(&self.connection).await?;
        if base.source.owner != current.owner || base.source.generation != current.generation {
            *self.baseline.lock().await = None;
            return Ok(());
        }
        let lost = lost_outputs(&base, &current);
        let mut records = self.locus.list("").await?;
        let returning = base.session.workspaces.iter().any(|w| {
            links::preferred_output(&records, &w.id)
                .is_some_and(|(output, _, fallback)| fallback && current.outputs.contains(&output))
        });
        if lost.is_empty() && !returning {
            return Ok(());
        }
        let focused = if let Some(focused) = base.recovery_focus.clone() {
            focused
        } else {
            let manager = workspace_dbus_api::ManagerProxy::builder(&self.connection)
                .destination(current.owner.clone())?
                .build()
                .await?;
            let focused = if let Some(path) = manager.focused_window().await?.first() {
                workspace_dbus_api::WindowProxy::builder(&self.connection)
                    .destination(current.owner.clone())?
                    .path(path.clone())?
                    .build()
                    .await?
                    .id()
                    .await
                    .ok()
            } else {
                None
            };
            if let Some(baseline) = self.baseline.lock().await.as_mut() {
                baseline.recovery_focus = Some(focused.clone());
            }
            focused
        };
        if !lost.is_empty() {
            let mut checkpoint = base.session.clone();
            checkpoint.name = CHECKPOINT.into();
            self.store.lock().await.put(checkpoint)?;
            for saved in &base.session.workspaces {
                if lost.contains(saved.output.as_str())
                    || base
                        .slots
                        .get(&saved.id)
                        .and_then(|id| base.source.workspaces.get(id))
                        .is_some_and(|w| lost.contains(w.output.as_str()))
                {
                    let (output, index, _) = links::preferred_output(&records, &saved.id)
                        .unwrap_or_else(|| (saved.output.clone(), saved.index, false));
                    links::set_preferred_output(
                        &self.locus,
                        &records,
                        &saved.id,
                        &output,
                        index,
                        true,
                    )
                    .await?;
                }
            }
            records = self.locus.list("").await?;
        }
        let recovery = plan(&base, &current, &records)?;
        if !recovery.workspaces.is_empty() {
            // Re-establish only identities captured in this same source epoch.
            for window in recovery.workspaces.iter().flat_map(|w| &w.windows) {
                let native = &base.windows[&window.id];
                links::set(
                    &self.locus,
                    &records,
                    links::LIVE_WINDOW,
                    RelationEndpoint::stable_key(locus::keys::WINDOW_ID, native),
                    RelationEndpoint::stable_key(links::SAVED_WINDOW, &window.id),
                    links::metadata(Some(&current)),
                    false,
                )
                .await?;
            }
            let report = self.restore_existing_groups(recovery).await?;
            anyhow::ensure!(
                report.issues.is_empty(),
                "output regrouping deferred: {}",
                report.issues.join("; ")
            );
        }
        if let Some(focused) = focused {
            let after = observe::read(&self.connection).await?;
            if after.windows.contains_key(&focused) {
                let control = workspace_dbus_api::ControlProxy::builder(&self.connection)
                    .destination(current.owner.clone())?
                    .build()
                    .await?;
                if control
                    .capabilities()
                    .await?
                    .iter()
                    .any(|c| c == "focus-window")
                {
                    control.focus_window(current.generation, &focused).await?;
                }
            }
        }
        let after = observe::read(&self.connection).await?;
        // Another output can disappear while returning groups are being moved
        // (for example closing the lid immediately after docking). Keep the
        // original baseline so that loss is reconciled on the next signal;
        // adopting `after` here would silently accept its evacuation merges.
        anyhow::ensure!(
            after.owner == current.owner
                && after.generation == current.generation
                && after.outputs == current.outputs,
            "source topology changed during regrouping; retaining the original baseline"
        );
        let records = self.locus.list("").await?;
        for saved in &base.session.workspaces {
            if let Some((output, index, true)) = links::preferred_output(&records, &saved.id) {
                let resolved = records.iter().any(|record| {
                    record.relation == links::LIVE_SLOT
                        && record.target == RelationEndpoint::stable_key(links::SLOT, &saved.id)
                        && links::current(record, &after)
                        && links::key(&record.subject, locus::keys::WORKSPACE_ID)
                            .and_then(|id| after.workspaces.get(&id))
                            .is_some_and(|w| w.output == output && after.outputs.contains(&output))
                });
                if resolved {
                    links::set_preferred_output(
                        &self.locus,
                        &records,
                        &saved.id,
                        &output,
                        index,
                        false,
                    )
                    .await?;
                }
            }
        }
        // Keep a valid baseline through the gap before the next capture. A
        // second topology change can arrive while the first is being repaired.
        let slots = records
            .iter()
            .filter(|r| r.relation == links::LIVE_SLOT && links::current(r, &after))
            .filter_map(|r| {
                Some((
                    links::key(&r.target, links::SLOT)?,
                    links::key(&r.subject, locus::keys::WORKSPACE_ID)?,
                ))
            })
            .filter(|(_, id)| {
                after
                    .workspaces
                    .get(id)
                    .is_some_and(|w| after.outputs.contains(&w.output))
            })
            .collect();
        *self.baseline.lock().await = Some(Baseline {
            source: after,
            session: base.session,
            slots,
            windows: base.windows,
            recovery_focus: None,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests;
