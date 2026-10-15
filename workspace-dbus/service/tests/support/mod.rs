use niri_ipc::{Event, Output, Reply, Request, Response, Window, WindowLayout, Workspace};
use std::{collections::HashMap, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
    sync::{RwLock, broadcast, watch},
    task::JoinHandle,
};

pub struct Source {
    pub events: broadcast::Sender<Event>,
    pub disconnect: watch::Sender<bool>,
    pub outputs: Arc<RwLock<HashMap<String, Output>>>,
    task: JoinHandle<()>,
}
impl Drop for Source {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Source {
    pub fn start(listener: UnixListener) -> Self {
        let (events, _) = broadcast::channel::<Event>(32);
        let (disconnect, _) = watch::channel(false);
        let outputs = Arc::new(RwLock::new(HashMap::from([(
            "eDP-1".into(),
            output("eDP-1"),
        )])));
        let event_sender = events.clone();
        let disconnect_sender = disconnect.clone();
        let shared_outputs = outputs.clone();
        let task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let mut events = event_sender.subscribe();
                let mut disconnect = disconnect_sender.subscribe();
                let outputs = shared_outputs.clone();
                tokio::spawn(async move {
                    let mut stream = BufReader::new(stream);
                    let mut line = String::new();
                    stream.read_line(&mut line).await.unwrap();
                    match serde_json::from_str::<Request>(&line).unwrap() {
                        Request::Outputs => {
                            let reply: Reply = Ok(Response::Outputs(outputs.read().await.clone()));
                            write(stream.get_mut(), &reply).await;
                        }
                        Request::EventStream => {
                            if *disconnect.borrow() {
                                return;
                            }
                            write(stream.get_mut(), &Ok::<_, String>(Response::Handled)).await;
                            // Windows first, and no keyboard events: readiness must be
                            // based on complete inventories, not timing or keyboards.
                            write(
                                stream.get_mut(),
                                &Event::WindowsChanged {
                                    windows: vec![window("initial")],
                                },
                            )
                            .await;
                            write(
                                stream.get_mut(),
                                &Event::WorkspacesChanged {
                                    workspaces: vec![workspace("eDP-1")],
                                },
                            )
                            .await;
                            loop {
                                tokio::select! {
                                    event = events.recv() => {
                                        let Ok(event) = event else { return };
                                        if stream.get_mut().write_all(&line_for(&event)).await.is_err() { return; }
                                    }
                                    _ = disconnect.changed() => if *disconnect.borrow() { return; },
                                }
                            }
                        }
                        request => panic!("unexpected native request: {request:?}"),
                    }
                });
            }
        });
        Self {
            events,
            disconnect,
            outputs,
            task,
        }
    }
}
fn line_for(value: &impl serde::Serialize) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(value).unwrap();
    bytes.push(b'\n');
    bytes
}
async fn write(stream: &mut tokio::net::UnixStream, value: &impl serde::Serialize) {
    stream.write_all(&line_for(value)).await.unwrap();
}
pub fn output(name: &str) -> Output {
    Output {
        name: name.into(),
        make: "Test".into(),
        model: "Panel".into(),
        serial: None,
        physical_size: None,
        modes: vec![],
        current_mode: None,
        is_custom_mode: false,
        vrr_supported: false,
        vrr_enabled: false,
        logical: None,
    }
}
pub fn workspace(output: &str) -> Workspace {
    Workspace {
        id: 5,
        idx: 2,
        name: Some("dev".into()),
        output: Some(output.into()),
        is_urgent: false,
        is_active: true,
        is_focused: true,
        active_window_id: Some(12),
    }
}
pub fn window(title: &str) -> Window {
    Window {
        id: 12,
        title: Some(title.into()),
        app_id: Some("test-app".into()),
        pid: Some(-1),
        workspace_id: Some(5),
        is_focused: true,
        is_floating: false,
        is_urgent: false,
        layout: WindowLayout {
            pos_in_scrolling_layout: Some((1, 1)),
            tile_size: (800.0, 600.0),
            window_size: (800, 600),
            tile_pos_in_workspace_view: None,
            window_offset_in_tile: (0.0, 0.0),
        },
        focus_timestamp: None,
    }
}
