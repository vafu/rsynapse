use crate::{
    backend::{Backend, BackendKind},
    dbus::{self, State},
    model::Projection,
};
use std::{path::PathBuf, sync::Arc, time::Duration};
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
    loop {
        if let Err(error) = host.session(&config, backend).await {
            tracing::warn!("source connection ended: {error:#}");
        }
        let current = host.state.read().await.clone();
        if current.manager.connected {
            let mut empty = Projection::default();
            empty.manager.generation = current.manager.generation.wrapping_add(1);
            host.publish(empty).await?;
        }
        // Retry backoff only after transport loss; live state is event-driven.
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
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

    async fn publish(&self, next: Projection) -> anyhow::Result<()> {
        let before = self.state.read().await.clone();
        if before == next {
            return Ok(());
        }
        let staged = before.stage_additions(&next);
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
            changed!(b, a, iface, ctx, focused => focused_changed, current_workspace => current_workspace_changed, workspaces => workspaces_changed);
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
        changed!(&before.manager, &after.manager, iface, ctx, generation => generation_changed, outputs => outputs_changed, workspaces => workspaces_changed, windows => windows_changed, focused_output => focused_output_changed, focused_workspace => focused_workspace_changed, focused_window => focused_window_changed, connected => connected_changed);
        Ok(())
    }
}
