//! Typed reads pinned to a unique owner. Watch connections are separate from
//! request connections so bounded signal backpressure cannot block replies.
use futures_util::StreamExt;
use std::collections::{BTreeMap, HashMap};
use workspace_dbus_api::{Layout, ManagerProxy, OutputProxy, WindowProxy, WorkspaceProxy};
use zbus::{Connection, fdo::DBusProxy, zvariant::OwnedObjectPath};

#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub id: String,
    pub app: String,
    pub pid: Option<i32>,
    pub workspace: String,
    pub floating: bool,
    pub column: Vec<u64>,
    pub row: Vec<u64>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Workspace {
    pub id: String,
    pub output: String,
    pub index: u32,
    pub name: String,
    pub layout: Layout,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub owner: String,
    pub generation: u64,
    pub workspaces: BTreeMap<String, Workspace>,
    pub windows: BTreeMap<String, Window>,
}

#[derive(Debug)]
pub struct Changed;
impl std::fmt::Display for Changed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("source changed while saving")
    }
}
impl std::error::Error for Changed {}

pub async fn read(connection: &Connection) -> anyhow::Result<Snapshot> {
    let watch_connection = Connection::session().await?;
    let mut events = watch(&watch_connection).await?;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            match read_once(connection).await {
                Ok(snapshot) => return Ok(snapshot),
                Err(error) => {
                    eprintln!("[steward/session] awaiting coherent inventory: {error:#}");
                    events
                        .next()
                        .await
                        .ok_or_else(|| anyhow::anyhow!("inventory stream ended"))??;
                }
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("no connected, coherent workspace inventory"))?
}

async fn read_once(connection: &Connection) -> anyhow::Result<Snapshot> {
    let bus = DBusProxy::new(connection).await?;
    let owner = bus
        .get_name_owner(workspace_dbus_api::BUS_NAME.try_into()?)
        .await?
        .to_string();
    let manager = ManagerProxy::builder(connection)
        .destination(owner.clone())?
        .build()
        .await?;
    anyhow::ensure!(
        manager.connected().await?,
        "workspace source is disconnected"
    );
    let generation = manager.generation().await?;
    let paths = manager.workspaces().await?;
    let window_paths = manager.windows().await?;
    let mut workspaces = BTreeMap::new();
    let mut identities = HashMap::<OwnedObjectPath, String>::new();
    for path in &paths {
        let proxy = WorkspaceProxy::builder(connection)
            .destination(owner.clone())?
            .path(path.clone())?
            .build()
            .await?;
        let id = proxy.id().await?;
        let output = if let Some(output) = proxy.output().await?.first() {
            OutputProxy::builder(connection)
                .destination(owner.clone())?
                .path(output.clone())?
                .build()
                .await?
                .name()
                .await?
        } else {
            continue;
        };
        identities.insert(path.clone(), id.clone());
        workspaces.insert(
            id.clone(),
            Workspace {
                id,
                output,
                index: proxy.index().await?,
                name: proxy.name().await?.first().cloned().unwrap_or_default(),
                layout: proxy.layout().await?,
            },
        );
    }
    let mut windows = BTreeMap::new();
    for path in &window_paths {
        let proxy = WindowProxy::builder(connection)
            .destination(owner.clone())?
            .path(path.clone())?
            .build()
            .await?;
        let Some(workspace) = proxy
            .workspace()
            .await?
            .first()
            .and_then(|p| identities.get(p))
            .cloned()
        else {
            continue;
        };
        let id = proxy.id().await?;
        windows.insert(
            id.clone(),
            Window {
                id,
                workspace,
                app: proxy.app_id().await?.first().cloned().unwrap_or_default(),
                pid: proxy.pid().await?.first().copied(),
                floating: proxy.floating().await?,
                column: proxy.column_index().await?,
                row: proxy.row_index().await?,
            },
        );
    }
    let connected = manager.connected().await?;
    let after_generation = manager.generation().await?;
    let after_paths = manager.workspaces().await?;
    let after_windows = manager.windows().await?;
    let after_owner = bus
        .get_name_owner(workspace_dbus_api::BUS_NAME.try_into()?)
        .await?;
    anyhow::ensure!(
        connected
            && after_generation == generation
            && after_paths == paths
            && after_windows == window_paths
            && after_owner.as_str() == owner,
        "workspace inventory changed during capture: connected={connected}, generation={generation}->{after_generation}, workspaces={paths:?}->{after_paths:?}, windows={window_paths:?}->{after_windows:?}, owner={owner}->{after_owner}"
    );
    Ok(Snapshot {
        owner,
        generation,
        workspaces,
        windows,
    })
}

pub async fn watch(connection: &Connection) -> anyhow::Result<zbus::MessageStream> {
    Ok(zbus::MessageStream::for_match_rule(
        zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(workspace_dbus_api::BUS_NAME)?
            .path_namespace(workspace_dbus_api::ROOT_PATH)?
            .build(),
        connection,
        Some(256),
    )
    .await?)
}
