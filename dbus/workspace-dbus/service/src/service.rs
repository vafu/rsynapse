use crate::model::Workspace;
use crate::{
    backend::{Backend, BackendKind},
    dbus::{self, State},
    model::Projection,
};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::RwLock;
use workspace_dbus_api::{BUS_NAME, ROOT_PATH, output_path, window_path, workspace_path};
use zbus::{Connection, connection::Builder};

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub backend: BackendKind,
    pub socket: Option<PathBuf>,
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    let backend = config.backend.resolve()?;
    let state: State = Arc::new(RwLock::new(Projection::default()));
    let (changed, changes) = tokio::sync::watch::channel(0u64);
    let connection = Builder::session()?
        .serve_at(ROOT_PATH, dbus::Manager(state.clone()))?
        .serve_at(
            ROOT_PATH,
            crate::control::Control {
                state: state.clone(),
                kind: backend,
                socket: config.socket.clone(),
                changed: changes,
                lock: Arc::new(tokio::sync::Mutex::new(())),
            },
        )?
        .serve_at(ROOT_PATH, zbus::fdo::ObjectManager)?
        .name(BUS_NAME)?
        .build()
        .await?;
    let host = Host {
        connection,
        state,
        changed,
    };
    tracing::info!(?backend, "owning {BUS_NAME}");
    let mut backoff = Duration::from_secs(1);
    loop {
        match host.session(&config, backend).await {
            Ok(()) => backoff = Duration::from_secs(1),
            Err(error) => {
                tracing::warn!("source connection ended: {error:#}; retrying in {backoff:?}");
                backoff = (backoff * 2).min(Duration::from_secs(30));
            }
        }
        let current = host.state.read().await.clone();
        if current.manager.connected {
            let mut empty = Projection::default();
            empty.manager.generation = current.manager.generation.wrapping_add(1);
            host.publish(empty).await?;
        }
        // Retry backoff only after transport loss; live state is event-driven.
        tokio::time::sleep(backoff).await;
    }
}

fn mru_ids(snapshot: &Projection) -> Vec<String> {
    snapshot
        .manager
        .workspace_mru
        .iter()
        .filter_map(|path| {
            snapshot
                .workspaces
                .keys()
                .find(|id| workspace_path(id.as_str()) == *path)
                .cloned()
        })
        .collect()
}

fn focused_workspace_id(snapshot: &Projection) -> Option<String> {
    let path = snapshot.manager.focused_workspace.first()?;
    snapshot
        .workspaces
        .keys()
        .find(|id| workspace_path(id.as_str()) == *path)
        .cloned()
}

fn update_workspace_mru(
    previous: Vec<String>,
    focused: Option<&str>,
    workspaces: &BTreeMap<String, Workspace>,
) -> Vec<String> {
    let mut mru = previous
        .into_iter()
        .filter(|id| workspaces.contains_key(id))
        .collect::<Vec<_>>();
    if let Some(focused) = focused {
        mru.retain(|id| id != focused);
        mru.insert(0, focused.to_owned());
    }
    for id in workspaces.keys() {
        if !mru.iter().any(|existing| existing == id) {
            mru.push(id.clone());
        }
    }
    mru
}

struct Host {
    connection: Connection,
    state: State,
    changed: tokio::sync::watch::Sender<u64>,
}

macro_rules! changed {
    ($before:expr, $after:expr, $iface:expr, $ctx:expr, $( $field:ident => $signal:ident ),+ $(,)?) => {
        $(if $before.$field != $after.$field { $iface.$signal($ctx).await?; })+
    };
}

impl Host {
    async fn session(&self, config: &Config, kind: BackendKind) -> anyhow::Result<()> {
        let mut backend = Backend::connect(kind, config.socket.as_deref()).await?;
        let generation = self.state.read().await.manager.generation.wrapping_add(1);
        loop {
            let mut projection = backend.next().await?;
            projection.manager.generation = generation;
            self.publish(projection).await?;
        }
    }

    async fn publish(&self, mut next: Projection) -> anyhow::Result<()> {
        let before = self.state.read().await.clone();
        let focused = focused_workspace_id(&next);
        let mru = update_workspace_mru(mru_ids(&before), focused.as_deref(), &next.workspaces);
        next.manager.workspace_mru = mru.iter().map(|id| workspace_path(id)).collect();
        if before == next {
            return Ok(());
        }
        let mut staged = before.stage_additions(&next);
        staged.manager.workspace_mru = next.manager.workspace_mru.clone();
        *self.state.write().await = staged.clone();
        for id in next
            .outputs
            .keys()
            .filter(|id| !before.outputs.contains_key(*id))
        {
            self.connection
                .object_server()
                .at(
                    output_path(id),
                    dbus::Output {
                        state: self.state.clone(),
                        id: id.clone(),
                    },
                )
                .await?;
        }
        for id in next
            .workspaces
            .keys()
            .filter(|id| !before.workspaces.contains_key(*id))
        {
            self.connection
                .object_server()
                .at(
                    workspace_path(id),
                    dbus::Workspace {
                        state: self.state.clone(),
                        id: id.clone(),
                    },
                )
                .await?;
        }
        for id in next
            .windows
            .keys()
            .filter(|id| !before.windows.contains_key(*id))
        {
            self.connection
                .object_server()
                .at(
                    window_path(id),
                    dbus::Window {
                        state: self.state.clone(),
                        id: id.clone(),
                    },
                )
                .await?;
        }
        *self.state.write().await = next.clone();
        self.emit_changes(&staged, &next).await?;
        self.changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
        for id in before
            .windows
            .keys()
            .filter(|id| !next.windows.contains_key(*id))
        {
            self.connection
                .object_server()
                .remove::<dbus::Window, _>(window_path(id))
                .await?;
        }
        for id in before
            .workspaces
            .keys()
            .filter(|id| !next.workspaces.contains_key(*id))
        {
            self.connection
                .object_server()
                .remove::<dbus::Workspace, _>(workspace_path(id))
                .await?;
        }
        for id in before
            .outputs
            .keys()
            .filter(|id| !next.outputs.contains_key(*id))
        {
            self.connection
                .object_server()
                .remove::<dbus::Output, _>(output_path(id))
                .await?;
        }
        Ok(())
    }

    async fn emit_changes(&self, before: &Projection, after: &Projection) -> zbus::Result<()> {
        // References are published only after all targets have been registered.
        for (id, a) in &after.outputs {
            let b = &before.outputs[id];
            let r = self
                .connection
                .object_server()
                .interface::<_, dbus::Output>(output_path(id))
                .await?;
            let iface = r.get().await;
            let ctx = r.signal_context();
            changed!(b, a, iface, ctx, enabled => enabled_changed, focused => focused_changed, current_workspace => current_workspace_changed, workspaces => workspaces_changed);
        }
        for (id, a) in &after.workspaces {
            let b = &before.workspaces[id];
            let r = self
                .connection
                .object_server()
                .interface::<_, dbus::Workspace>(workspace_path(id))
                .await?;
            let iface = r.get().await;
            let ctx = r.signal_context();
            changed!(b, a, iface, ctx, layout => layout_changed, name => name_changed, index => index_changed, output => output_changed, active => active_changed, focused => focused_changed, urgent => urgent_changed, active_window => active_window_changed, windows => windows_changed);
        }
        for (id, a) in &after.windows {
            let b = &before.windows[id];
            let r = self
                .connection
                .object_server()
                .interface::<_, dbus::Window>(window_path(id))
                .await?;
            let iface = r.get().await;
            let ctx = r.signal_context();
            changed!(b, a, iface, ctx, title => title_changed, app_id => app_id_changed, pid => pid_changed, workspace => workspace_changed, output => output_changed, focused => focused_changed, floating => floating_changed, urgent => urgent_changed, column_index => column_index_changed, row_index => row_index_changed);
        }
        let r = self
            .connection
            .object_server()
            .interface::<_, dbus::Manager>(ROOT_PATH)
            .await?;
        let iface = r.get().await;
        let ctx = r.signal_context();
        changed!(&before.manager, &after.manager, iface, ctx, generation => generation_changed, outputs => outputs_changed, workspaces => workspaces_changed, windows => windows_changed, focused_output => focused_output_changed, focused_workspace => focused_workspace_changed, focused_window => focused_window_changed, workspace_mru => workspace_mru_changed, connected => connected_changed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspaces(ids: &[&str]) -> BTreeMap<String, Workspace> {
        ids.iter()
            .map(|id| ((*id).to_owned(), Workspace::default()))
            .collect()
    }

    #[test]
    fn mru_moves_focused_workspace_to_front() {
        let mru = update_workspace_mru(
            vec!["a".into(), "b".into(), "c".into()],
            Some("b"),
            &workspaces(&["a", "b", "c"]),
        );
        assert_eq!(mru, vec!["b", "a", "c"]);
    }

    #[test]
    fn mru_drops_missing_workspaces_and_appends_new_ones() {
        let mru = update_workspace_mru(
            vec!["old".into(), "b".into()],
            None,
            &workspaces(&["a", "b", "c"]),
        );
        assert_eq!(mru, vec!["b", "a", "c"]);
    }

    #[test]
    fn mru_current_workspace_is_always_first() {
        let mru = update_workspace_mru(
            vec!["a".into(), "b".into(), "c".into()],
            Some("c"),
            &workspaces(&["a", "b", "c"]),
        );
        assert_eq!(mru.first().map(String::as_str), Some("c"));
        assert_eq!(mru, vec!["c", "a", "b"]);
    }
}
