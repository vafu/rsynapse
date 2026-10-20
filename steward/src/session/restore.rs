use super::{
    Manager, launch, links,
    model::{self, RestoreReport, Session},
    observe,
};
use crate::relations::WORKSPACE_PROJECT;
use futures_util::StreamExt;
use locus::{RelationEndpoint, RelationRecord};
use std::{collections::BTreeSet, process::Stdio, time::Duration};
use workspace_dbus_api::ControlProxy;

const PENDING: &str = "org.rsynapse.session.window.launch";

impl Manager {
    pub(super) async fn restore(&self, session: Session) -> anyhow::Result<RestoreReport> {
        self.restore_inner(session, true).await
    }

    pub(super) async fn restore_existing_groups(
        &self,
        session: Session,
    ) -> anyhow::Result<RestoreReport> {
        anyhow::ensure!(
            session
                .workspaces
                .iter()
                .flat_map(|w| &w.windows)
                .all(|w| w.recipe.is_empty()),
            "hotplug regrouping must not contain launch recipes"
        );
        self.restore_inner(session, false).await
    }

    async fn restore_inner(&self, session: Session, arm: bool) -> anyhow::Result<RestoreReport> {
        session.validate()?;
        let source = observe::read(&self.connection).await?;
        let control = ControlProxy::builder(&self.connection)
            .destination(source.owner.as_str())?
            .build()
            .await?;
        let capabilities = control.capabilities().await?;
        anyhow::ensure!(
            ["select-workspace", "set-layout", "place-window"]
                .iter()
                .all(|name| capabilities.iter().any(|c| c == name)),
            "compositor backend does not support session restoration"
        );
        // Native silent moves still require focusing their source window. Keep
        // the user's original window so explicit restoration returns there,
        // including when only part of a session could be restored. Hotplug
        // reconciliation preserves focus across its own retries separately.
        let focused = if arm && capabilities.iter().any(|name| name == "focus-window") {
            let manager = workspace_dbus_api::ManagerProxy::builder(&self.connection)
                .destination(source.owner.as_str())?
                .build()
                .await?;
            match manager.focused_window().await?.first() {
                Some(path) => Some(
                    workspace_dbus_api::WindowProxy::builder(&self.connection)
                        .destination(source.owner.clone())?
                        .path(path.clone())?
                        .build()
                        .await?
                        .id()
                        .await?,
                ),
                None => None,
            }
        } else {
            None
        };
        let projects: Vec<goal_model::ProjectInfo> =
            self.projd().await?.call("ListProjects", &()).await?;
        let mut report = RestoreReport::default();
        let mut used = BTreeSet::new();
        for workspace in &session.workspaces {
            let result = self
                .restore_workspace(
                    &source,
                    &control,
                    &projects,
                    workspace,
                    &mut used,
                    &mut report,
                )
                .await;
            if let Err(error) = result {
                report
                    .issues
                    .push(format!("workspace {}: {error:#}", workspace.id));
            }
        }
        let after = match observe::read(&self.connection).await {
            Ok(after) if after.owner == source.owner && after.generation == source.generation => {
                after
            }
            _ => {
                report.issues.push("source reset during restoration".into());
                return Ok(report);
            }
        };
        if let Some(focused) = focused.filter(|id| after.windows.contains_key(id)) {
            if let Err(error) = control.focus_window(source.generation, &focused).await {
                report.issues.push(format!(
                    "could not return focus to the original window: {error}"
                ));
            }
        }
        // A partial restore leaves the input checkpoint protected. Successfully
        // restored window anchors still make a repeated command idempotent.
        if arm && report.issues.is_empty() {
            links::arm(&self.locus, &after).await?;
        }
        Ok(report)
    }

    async fn restore_workspace(
        &self,
        source: &observe::Snapshot,
        control: &ControlProxy<'_>,
        projects: &[goal_model::ProjectInfo],
        workspace: &model::Workspace,
        used: &mut BTreeSet<String>,
        report: &mut RestoreReport,
    ) -> anyhow::Result<()> {
        let records = self.locus.list("").await?;
        let project_id = links::target(
            &records,
            links::CHECKPOINT_PROJECT,
            links::CHECKPOINT,
            &workspace.binding,
        )
        .and_then(|r| links::key(&r.target, links::PROJECT));
        let project = project_id
            .as_ref()
            .map(|id| {
                projects
                    .iter()
                    .find(|p| &p.id == id)
                    .cloned()
                    .ok_or_else(|| {
                        anyhow::anyhow!("saved project was removed; it will not be recreated")
                    })
            })
            .transpose()?;
        if let Some(project) = &project {
            links::set(
                &self.locus,
                &records,
                links::SLOT_PROJECT,
                RelationEndpoint::stable_key(links::SLOT, &workspace.id),
                RelationEndpoint::stable_key(links::PROJECT, &project.id),
                links::metadata(None),
                true,
            )
            .await?;
        }
        let current = observe::read(&self.connection).await?;
        anyhow::ensure!(
            current.owner == source.owner && current.generation == source.generation,
            "source epoch changed"
        );
        let output = if current.outputs.contains(&workspace.output) {
            workspace.output.clone()
        } else {
            current
                .outputs
                .iter()
                .next()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("no live output"))?
        };
        let existing = current.workspaces.values().find(|w| {
            w.output == output
                && links::target(&records, links::LIVE_SLOT, locus::keys::WORKSPACE_ID, &w.id)
                    .is_some_and(|r| {
                        links::current(r, &current)
                            && links::key(&r.target, links::SLOT).as_deref() == Some(&workspace.id)
                    })
        });
        let named = current
            .workspaces
            .values()
            .find(|w| !workspace.name.is_empty() && w.name == workspace.name && w.output == output);
        let blank = current
            .workspaces
            .values()
            .filter(|w| {
                w.output == output
                    && w.name.is_empty()
                    && !used.contains(&w.id)
                    && !current
                        .windows
                        .values()
                        .any(|window| window.workspace == w.id)
                    && links::target(
                        &records,
                        WORKSPACE_PROJECT,
                        locus::keys::WORKSPACE_ID,
                        &w.id,
                    )
                    .is_none()
            })
            // Dynamic inventories retain the trailing blank. An interior
            // anonymous blank may be pruned when focusing the source window.
            .max_by_key(|w| w.index);
        let candidate = existing
            .or(named)
            .or(blank)
            .ok_or_else(|| anyhow::anyhow!("no unassigned workspace available on {output}"))?;
        if workspace.windows.is_empty() && existing.is_none() && named.is_none() {
            anyhow::bail!(
                "empty logical workspace preserved; configure a persistent named/static workspace to materialize it on Umbriel"
            );
        }
        // PlaceWindow addresses the opaque destination directly. Selecting an
        // empty workspace first starts a native slide before it is populated,
        // delaying creation of the next blank.
        let live = candidate.id.clone();
        anyhow::ensure!(
            !used.contains(&live),
            "workspace already reserved by another saved workspace"
        );
        if let Some(binding) = links::target(
            &records,
            WORKSPACE_PROJECT,
            locus::keys::WORKSPACE_ID,
            &live,
        ) {
            anyhow::ensure!(
                binding.metadata.get("project-id") == project.as_ref().map(|p| &p.id),
                "existing explicit project binding conflicts with restore"
            );
        }
        used.insert(live.clone());
        links::bind_slot(&self.locus, &current, &workspace.id, &live).await?;
        crate::associations::restore_workspace(&self.locus, &live, project.clone()).await?;
        for saved in &workspace.windows {
            let mut window = saved.clone();
            if let Some(project) = &project {
                for recipe in &mut window.recipe {
                    recipe.cwd = project.cwd.clone();
                }
            }
            let result = self
                .restore_window(source, control, &live, &window, &records, report)
                .await;
            if let Err(error) = result {
                report
                    .issues
                    .push(format!("{} ({}): {error:#}", window.app_id, window.id));
            }
        }
        if workspace.layout != workspace_dbus_api::Layout::Unknown {
            control
                .set_layout(source.generation, &live, workspace.layout)
                .await?;
        }
        Ok(())
    }

    async fn restore_window(
        &self,
        source: &observe::Snapshot,
        control: &ControlProxy<'_>,
        workspace: &str,
        window: &model::Window,
        records: &[RelationRecord],
        report: &mut RestoreReport,
    ) -> anyhow::Result<()> {
        let current = observe::read(&self.connection).await?;
        if let Some(existing) = current.windows.values().find(|w| {
            links::target(records, links::LIVE_WINDOW, locus::keys::WINDOW_ID, &w.id).is_some_and(
                |r| {
                    links::current(r, &current)
                        && links::key(&r.target, links::SAVED_WINDOW).as_deref() == Some(&window.id)
                },
            )
        }) {
            control
                .place_window(source.generation, &existing.id, workspace, window.floating)
                .await?;
            report.reused += 1;
            return Ok(());
        }
        let recipe = window.recipe.first().ok_or_else(|| {
            anyhow::anyhow!("no launch recipe; configure this application's launcher")
        })?;
        let pending = links::target(records, PENDING, links::SAVED_WINDOW, &window.id)
            .filter(|r| links::current(r, &current));
        if let Some(pending) = pending {
            let app = pending.metadata.get("app-id").cloned().unwrap_or_default();
            let found: Vec<_> = current
                .windows
                .values()
                .filter(|w| {
                    w.app == app
                        && pending
                            .metadata
                            .get("token-match")
                            .is_some_and(|v| v == "true")
                })
                .collect();
            anyhow::ensure!(
                found.len() == 1,
                "previous launch is unresolved; refusing to launch a duplicate"
            );
            self.remember_window(source, &window.id, &found[0].id)
                .await?;
            control
                .place_window(source.generation, &found[0].id, workspace, window.floating)
                .await?;
            report.reused += 1;
            return Ok(());
        }
        let token = model::id();
        let (argv, expected) = launch::expanded(recipe, &token)?;
        let before: BTreeSet<_> = current.windows.keys().cloned().collect();
        let watch_connection = zbus::Connection::session().await?;
        let mut events = observe::watch(&watch_connection).await?;
        // Subscribe before launching. Match explicit app tokens, or the causal
        // fresh process plus app identity for ordinary desktop-entry launches.
        let mut metadata = links::metadata(Some(source));
        metadata.insert("app-id".into(), expected.clone());
        metadata.insert(
            "token-match".into(),
            recipe.match_app_id.contains("{restore_id}").to_string(),
        );
        self.locus
            .set_one_with_persistence(
                RelationEndpoint::stable_key(links::SAVED_WINDOW, &window.id),
                PENDING,
                RelationEndpoint::stable_key("org.rsynapse.session.launch.id", &token),
                metadata,
                false,
            )
            .await?;
        let child = tokio::process::Command::new(&argv[0])
            .args(&argv[1..])
            .current_dir(&recipe.cwd)
            // A restore is not a continuation of whichever launcher originally
            // started Steward. Do not forward an unrelated activation request.
            .env_remove("XDG_ACTIVATION_TOKEN")
            .env_remove("DESKTOP_STARTUP_ID")
            .env("RSYNAPSE_RESTORE_ID", &token)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(error) => {
                self.locus
                    .unset(
                        RelationEndpoint::stable_key(links::SAVED_WINDOW, &window.id),
                        PENDING,
                        RelationEndpoint::stable_key("org.rsynapse.session.launch.id", &token),
                    )
                    .await?;
                return Err(error.into());
            }
        };
        let pid = child
            .id()
            .ok_or_else(|| anyhow::anyhow!("launch has no process identity"))?;
        tokio::spawn(async move {
            let _ = child.wait().await;
        });
        let matched = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let snapshot = observe::read(&self.connection).await?;
                anyhow::ensure!(
                    snapshot.owner == source.owner && snapshot.generation == source.generation,
                    "source reset while launching"
                );
                let candidates: Vec<_> = snapshot
                    .windows
                    .values()
                    .filter(|w| {
                        !before.contains(&w.id)
                            && w.app == expected
                            && (recipe.match_app_id.contains("{restore_id}")
                                || w.pid.is_some_and(|p| launch::descendant(p, pid)))
                    })
                    .collect();
                anyhow::ensure!(
                    candidates.len() <= 1,
                    "launch created multiple indistinguishable windows"
                );
                if let Some(window) = candidates.first() {
                    return Ok::<_, anyhow::Error>(window.id.clone());
                }
                events
                    .next()
                    .await
                    .ok_or_else(|| anyhow::anyhow!("window event stream ended"))??;
            }
        })
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "no correlated window appeared (single-instance forwarding may be unsupported)"
            )
        })??;
        self.remember_window(source, &window.id, &matched).await?;
        control
            .place_window(source.generation, &matched, workspace, window.floating)
            .await?;
        report.restored += 1;
        Ok(())
    }

    async fn remember_window(
        &self,
        source: &observe::Snapshot,
        saved: &str,
        live: &str,
    ) -> anyhow::Result<()> {
        self.locus
            .set_one_with_persistence(
                RelationEndpoint::stable_key(locus::keys::WINDOW_ID, live),
                links::LIVE_WINDOW,
                RelationEndpoint::stable_key(links::SAVED_WINDOW, saved),
                links::metadata(Some(source)),
                false,
            )
            .await?;
        for record in self.locus.list(PENDING).await? {
            if record.subject == RelationEndpoint::stable_key(links::SAVED_WINDOW, saved)
                && links::current(&record, source)
            {
                self.locus
                    .unset(record.subject, PENDING, record.target)
                    .await?;
            }
        }
        Ok(())
    }
}
