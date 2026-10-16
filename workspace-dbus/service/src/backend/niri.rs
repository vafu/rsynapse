use crate::model::{self, Projection};
use niri_ipc::{
    Event, Output, Request, Response,
    state::{EventStreamState, EventStreamStatePart},
};
use std::{
    collections::HashMap,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
};
use workspace_dbus_api::{Layout, output_path, window_path, workspace_path};
mod socket;
use socket::Socket;

pub(crate) struct Niri {
    path: PathBuf,
    stream: Socket,
    outputs: HashMap<String, Output>,
    state: EventStreamState,
    workspaces_ready: bool,
    windows_ready: bool,
}

impl Niri {
    pub async fn connect(path: Option<&Path>) -> anyhow::Result<Self> {
        let path = path
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os("NIRI_SOCKET").map(PathBuf::from))
            .ok_or_else(|| anyhow::anyhow!("NIRI_SOCKET is not set; use --socket PATH"))?;
        let outputs = Self::outputs(&path).await?;
        let mut stream = Socket::connect(&path).await?;
        match stream.send(Request::EventStream).await? {
            Response::Handled => (),
            reply => anyhow::bail!("unexpected event stream response: {reply:?}"),
        }
        stream.finish_requests().await?;
        Ok(Self {
            path,
            stream,
            outputs,
            state: EventStreamState::default(),
            workspaces_ready: false,
            windows_ready: false,
        })
    }

    async fn outputs(path: &Path) -> anyhow::Result<HashMap<String, Output>> {
        match Socket::connect(path).await?.send(Request::Outputs).await? {
            Response::Outputs(outputs) => Ok(outputs),
            reply => anyhow::bail!("unexpected output response: {reply:?}"),
        }
    }

    pub async fn next(&mut self) -> anyhow::Result<Projection> {
        loop {
            let event = self.stream.event().await?;
            if matches!(event, Event::WorkspacesChanged { .. }) {
                self.outputs = Self::outputs(&self.path).await?;
                self.workspaces_ready = true;
            }
            if matches!(event, Event::WindowsChanged { .. }) {
                self.windows_ready = true;
            }
            catch_unwind(AssertUnwindSafe(|| self.state.apply(event)))
                .map_err(|_| anyhow::anyhow!("Niri rejected event ordering"))?;
            if self.workspaces_ready && self.windows_ready {
                return Ok(self.project());
            }
        }
    }

    fn project(&self) -> Projection {
        let mut result = Projection::default();
        result.manager.connected = true;
        let workspaces = &self.state.workspaces.workspaces;
        let windows = &self.state.windows.windows;
        let focused_window = windows.values().find(|w| w.is_focused);
        let focused_workspace = match focused_window {
            Some(window) => window.workspace_id.and_then(|id| workspaces.get(&id)),
            None => workspaces.values().find(|ws| ws.is_focused),
        };
        let focused_output = focused_workspace.and_then(|ws| ws.output.as_deref());
        result.manager.focused_window = focused_window
            .map(|w| window_path(&w.id.to_string()))
            .into_iter()
            .collect();
        result.manager.focused_workspace = focused_workspace
            .map(|ws| workspace_path(&ws.id.to_string()))
            .into_iter()
            .collect();
        result.manager.focused_output = focused_output.map(output_path).into_iter().collect();
        for name in self.outputs.keys() {
            let mut members: Vec<_> = workspaces
                .values()
                .filter(|ws| ws.output.as_ref() == Some(name))
                .collect();
            members.sort_by_key(|ws| (ws.idx, ws.id));
            result.outputs.insert(
                name.clone(),
                model::Output {
                    focused: focused_output == Some(name.as_str()),
                    current_workspace: members
                        .iter()
                        .find(|ws| ws.is_active)
                        .map(|ws| workspace_path(&ws.id.to_string()))
                        .into_iter()
                        .collect(),
                    workspaces: members
                        .iter()
                        .map(|ws| workspace_path(&ws.id.to_string()))
                        .collect(),
                },
            );
        }
        for ws in workspaces.values() {
            let mut members: Vec<_> = windows
                .values()
                .filter(|w| w.workspace_id == Some(ws.id))
                .collect();
            members.sort_by_key(|w| {
                (
                    w.layout
                        .pos_in_scrolling_layout
                        .unwrap_or((usize::MAX, usize::MAX)),
                    w.id,
                )
            });
            result.workspaces.insert(
                ws.id.to_string(),
                model::Workspace {
                    layout: Layout::Scrolling,
                    name: ws.name.clone().into_iter().collect(),
                    index: u32::from(ws.idx),
                    output: ws.output.as_deref().map(output_path).into_iter().collect(),
                    active: ws.is_active,
                    focused: focused_workspace.is_some_and(|focus| focus.id == ws.id),
                    urgent: ws.is_urgent,
                    active_window: ws
                        .active_window_id
                        .map(|id| window_path(&id.to_string()))
                        .into_iter()
                        .collect(),
                    windows: members
                        .iter()
                        .map(|w| window_path(&w.id.to_string()))
                        .collect(),
                },
            );
        }
        for w in windows.values() {
            result.windows.insert(
                w.id.to_string(),
                model::Window {
                    title: w.title.clone().into_iter().collect(),
                    app_id: w.app_id.clone().into_iter().collect(),
                    pid: w.pid.filter(|pid| *pid > 0).into_iter().collect(),
                    workspace: w
                        .workspace_id
                        .map(|id| workspace_path(&id.to_string()))
                        .into_iter()
                        .collect(),
                    output: w
                        .workspace_id
                        .and_then(|id| workspaces.get(&id))
                        .and_then(|ws| ws.output.as_deref())
                        .map(output_path)
                        .into_iter()
                        .collect(),
                    focused: w.is_focused,
                    floating: w.is_floating,
                    urgent: w.is_urgent,
                    // Niri uses one-based scrolling coordinates; the public contract is zero-based.
                    column_index: w
                        .layout
                        .pos_in_scrolling_layout
                        .and_then(|p| p.0.checked_sub(1))
                        .map(|v| v as u64)
                        .into_iter()
                        .collect(),
                    row_index: w
                        .layout
                        .pos_in_scrolling_layout
                        .and_then(|p| p.1.checked_sub(1))
                        .map(|v| v as u64)
                        .into_iter()
                        .collect(),
                },
            );
        }
        result.normalize();
        result
    }
}
