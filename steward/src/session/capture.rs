use super::{Manager, links, model, observe};
use locus::RelationEndpoint;
use std::collections::BTreeMap;

impl Manager {
    /// Called under the operation gate; immutable checkpoint links are written
    /// before publishing the description that references them.
    pub(super) async fn capture(
        &self,
        name: &str,
        explicit: bool,
    ) -> anyhow::Result<model::Session> {
        use futures_util::StreamExt;
        let connection = zbus::Connection::session().await?;
        let mut events = observe::watch(&connection).await?;
        let mut relations = zbus::MessageStream::for_match_rule(
            zbus::MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .sender(locus::BUS_NAME)?
                .build(),
            &connection,
            Some(256),
        )
        .await?;
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                self.reconcile_topology().await?;
                match self.capture_once(name, explicit).await {
                    Err(error) if error.is::<observe::Changed>() => {
                        tokio::select! {
                            event = events.next() => event,
                            event = relations.next() => event,
                        }
                        .ok_or_else(|| anyhow::anyhow!("capture stream ended"))??;
                    }
                    result => return result,
                }
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("source kept changing while saving"))?
    }

    async fn capture_once(&self, name: &str, explicit: bool) -> anyhow::Result<model::Session> {
        let source = observe::read(&self.connection).await?;
        let records = self.locus.list("").await?;
        if self
            .baseline
            .lock()
            .await
            .as_ref()
            .is_some_and(|base| super::topology::needs_reconciliation(base, &source, &records))
        {
            return Err(observe::Changed.into());
        }
        let projects: Vec<goal_model::ProjectInfo> =
            self.projd().await?.call("ListProjects", &()).await?;
        let previous = self.store.lock().await.get("current").ok();
        let old_scene = self.baseline.lock().await.clone();
        let home = std::env::var("HOME")?;
        let mut workspaces = Vec::new();
        let mut slots = BTreeMap::new();
        let mut identities = BTreeMap::new();
        for workspace in source.workspaces.values() {
            if !source.outputs.contains(&workspace.output) {
                continue;
            }
            let binding = links::target(
                &records,
                crate::relations::WORKSPACE_PROJECT,
                locus::keys::WORKSPACE_ID,
                &workspace.id,
            )
            .filter(|r| links::current(r, &source));
            let project = binding
                .and_then(|r| r.metadata.get("project-id"))
                .and_then(|id| projects.iter().find(|p| &p.id == id));
            let members: Vec<_> = source
                .windows
                .values()
                .filter(|w| w.workspace == workspace.id)
                .collect();
            // Automatic edge placeholders have no durable intent.
            if project.is_none() && members.is_empty() && workspace.name.is_empty() {
                continue;
            }
            let slot = links::live(
                &self.locus,
                &records,
                &source,
                links::LIVE_SLOT,
                locus::keys::WORKSPACE_ID,
                &workspace.id,
                links::SLOT,
            )
            .await?;
            slots.insert(slot.clone(), workspace.id.clone());
            let old_output = old_scene
                .as_ref()
                .and_then(|b| {
                    b.slots
                        .get(&slot)
                        .and_then(|native| b.source.workspaces.get(native))
                })
                .map(|w| w.output.as_str());
            let (preferred, position, fallback) = match links::preferred_output(&records, &slot) {
                Some((preferred, position, true))
                    if preferred != workspace.output
                        && old_output.is_none_or(|old| old == workspace.output) =>
                {
                    (preferred, position, true)
                }
                _ => (workspace.output.clone(), workspace.index, false),
            };
            links::set_preferred_output(
                &self.locus,
                &records,
                &slot,
                &preferred,
                position,
                fallback,
            )
            .await?;
            let subject = RelationEndpoint::stable_key(links::SLOT, &slot);
            if let Some(project) = project {
                links::set(
                    &self.locus,
                    &records,
                    links::SLOT_PROJECT,
                    subject,
                    RelationEndpoint::stable_key(links::PROJECT, &project.id),
                    links::metadata(None),
                    true,
                )
                .await?;
            } else if let Some(prior) =
                links::target(&records, links::SLOT_PROJECT, links::SLOT, &slot)
            {
                self.locus
                    .unset(
                        prior.subject.clone(),
                        links::SLOT_PROJECT,
                        prior.target.clone(),
                    )
                    .await?;
            }
            let old = previous
                .as_ref()
                .and_then(|s| s.workspaces.iter().find(|w| w.id == slot));
            let old_project = old
                .and_then(|w| {
                    links::target(
                        &records,
                        links::CHECKPOINT_PROJECT,
                        links::CHECKPOINT,
                        &w.binding,
                    )
                })
                .and_then(|r| links::key(&r.target, links::PROJECT));
            let checkpoint = if old_project.as_deref() == project.map(|p| p.id.as_str()) {
                old.map(|w| w.binding.clone()).unwrap_or_else(model::id)
            } else {
                model::id()
            };
            if let Some(project) = project {
                links::set(
                    &self.locus,
                    &records,
                    links::CHECKPOINT_PROJECT,
                    RelationEndpoint::stable_key(links::CHECKPOINT, &checkpoint),
                    RelationEndpoint::stable_key(links::PROJECT, &project.id),
                    links::metadata(None),
                    true,
                )
                .await?;
            }
            let mut windows = Vec::new();
            for window in members {
                let id = links::live(
                    &self.locus,
                    &records,
                    &source,
                    links::LIVE_WINDOW,
                    locus::keys::WINDOW_ID,
                    &window.id,
                    links::SAVED_WINDOW,
                )
                .await?;
                identities.insert(id.clone(), window.id.clone());
                // Keep captured recipes stable across daemon restarts and
                // restored app IDs; refreshing a save can pick up rule changes.
                let prior = previous
                    .as_ref()
                    .into_iter()
                    .flat_map(|s| &s.workspaces)
                    .flat_map(|w| &w.windows)
                    .find(|w| w.id == id);
                let app_id = prior
                    .map(|w| w.app_id.as_str())
                    .unwrap_or_else(|| super::launch::canonical(&window.app));
                let recipe = self
                    .rules
                    .recipe(app_id, project.map(|p| p.cwd.as_str()).unwrap_or(&home));
                windows.push(model::Window {
                    id,
                    app_id: app_id.into(),
                    floating: window.floating,
                    column: window.column.clone(),
                    row: window.row.clone(),
                    recipe: recipe.into_iter().collect(),
                });
            }
            windows.sort_by(|a, b| (&a.column, &a.row, &a.id).cmp(&(&b.column, &b.row, &b.id)));
            workspaces.push(model::Workspace {
                id: slot,
                binding: checkpoint,
                output: preferred,
                index: position,
                name: workspace.name.clone(),
                layout: workspace.layout,
                windows,
            });
        }
        workspaces.sort_by(|a, b| (&a.output, a.index).cmp(&(&b.output, b.index)));
        let session = model::Session {
            version: 1,
            name: name.into(),
            workspaces,
        };
        // Do not commit observations spanning a reset or disappearing objects.
        let after = observe::read(&self.connection).await?;
        if source != after {
            return Err(observe::Changed.into());
        }
        let bindings = |rows: &[locus::RelationRecord]| {
            rows.iter()
                .filter(|r| r.relation == crate::relations::WORKSPACE_PROJECT)
                .map(|r| (r.subject.clone(), (r.target.clone(), r.metadata.clone())))
                .collect::<BTreeMap<_, _>>()
        };
        if bindings(&records)
            != bindings(&self.locus.list(crate::relations::WORKSPACE_PROJECT).await?)
        {
            return Err(observe::Changed.into());
        }
        let mut store = self.store.lock().await;
        let armed = explicit
            || store.get("last").is_err()
            || records.iter().any(|r| {
                r.relation == links::CAPTURE
                    && r.target == links::epoch(&source)
                    && links::current(r, &source)
            });
        if explicit {
            store.put(session.clone())?;
        } else if !session.workspaces.is_empty() {
            store.put(session.clone())?;
            if armed {
                let mut last = session.clone();
                last.name = "last".into();
                store.put(last)?;
            }
        }
        drop(store);
        *self.baseline.lock().await = Some(super::topology::Baseline {
            source: source.clone(),
            session: session.clone(),
            slots,
            windows: identities,
            recovery_focus: None,
        });
        if explicit || armed {
            links::arm(&self.locus, &source).await?;
        }
        // Expired session-only anchors are never evidence in a new epoch.
        let live: BTreeMap<_, _> = source.windows.iter().map(|(id, _)| (id, ())).collect();
        for record in records.iter().filter(|r| {
            [links::LIVE_SLOT, links::LIVE_WINDOW, links::CAPTURE].contains(&r.relation.as_str())
        }) {
            let closed = record.relation == links::LIVE_WINDOW
                && links::key(&record.subject, locus::keys::WINDOW_ID)
                    .is_some_and(|id| !live.contains_key(&id));
            if !links::current(record, &source) || closed {
                self.locus
                    .unset(
                        record.subject.clone(),
                        &record.relation,
                        record.target.clone(),
                    )
                    .await?;
            }
        }
        Ok(session)
    }
}
