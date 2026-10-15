use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
    sync::{Notify, broadcast},
    task::JoinHandle,
};

pub const WINDOW: &str = "α/window:007";
pub const OTHER: &str = "0007";
pub const WORKSPACE: &str = "DP-A:1";
pub const OTHER_WORKSPACE: &str = "DP-B:1";

#[derive(Clone, Debug)]
pub enum Message {
    Frame(Value),
    Close,
}
pub struct Source {
    pub events: broadcast::Sender<Message>,
    pub subscribed: Arc<Notify>,
    hold: Arc<AtomicBool>,
    task: JoinHandle<()>,
}
impl Drop for Source {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Source {
    pub fn start(listener: UnixListener) -> Self {
        let (events, _) = broadcast::channel::<Message>(32);
        let subscribed = Arc::new(Notify::new());
        let hold = Arc::new(AtomicBool::new(true));
        let sender = events.clone();
        let notify = subscribed.clone();
        let bootstrap = hold.clone();
        let task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let mut events = sender.subscribe();
                let notify = notify.clone();
                let hold = bootstrap.clone();
                tokio::spawn(async move {
                    let (read, mut write) = stream.into_split();
                    let mut read = BufReader::new(read);
                    let mut request = String::new();
                    read.read_line(&mut request).await.unwrap();
                    assert_eq!(
                        serde_json::from_str::<Value>(&request).unwrap(),
                        json!({
                            "cmd":"subscribe", "events":["outputs","workspaces","windows","layers","overview"]
                        })
                    );
                    let mut initial = vec![
                        event("windows", windows("initial", true)),
                        event("outputs", outputs()),
                        event("layers", json!([])),
                        event("overview", json!({"open":false})),
                    ];
                    if !hold.load(Ordering::SeqCst) {
                        initial.push(event("workspaces", workspaces()));
                    }
                    // No reply acknowledgment, no keyboard, arbitrary family order,
                    // and several JSON frames in one fragmented socket write.
                    let bytes: Vec<_> = initial.iter().flat_map(line).collect();
                    write.write_all(&bytes[..9]).await.unwrap();
                    tokio::task::yield_now().await;
                    write.write_all(&bytes[9..]).await.unwrap();
                    notify.notify_one();
                    while let Ok(message) = events.recv().await {
                        match message {
                            Message::Frame(frame) => {
                                if write.write_all(&line(&frame)).await.is_err() {
                                    return;
                                }
                            }
                            Message::Close => return,
                        }
                    }
                });
            }
        });
        Self {
            events,
            subscribed,
            hold,
            task,
        }
    }
    pub fn send(&self, family: &str, data: Value) {
        self.events
            .send(Message::Frame(event(family, data)))
            .unwrap();
    }
    pub fn finish_bootstrap(&self) {
        self.hold.store(false, Ordering::SeqCst);
        self.send("workspaces", workspaces());
    }
}

fn line(value: &Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(value).unwrap();
    bytes.push(b'\n');
    bytes
}
pub fn event(family: &str, data: Value) -> Value {
    json!({"event":family,"data":data})
}
pub fn outputs() -> Value {
    json!([
        {"name":"DP-A","focused":false,"active_workspace":WORKSPACE},
        {"name":"DP-B","focused":true,"active_workspace":OTHER_WORKSPACE}
    ])
}
pub fn workspaces() -> Value {
    json!([
        {"id":WORKSPACE,"name":"1","named":false,"index":1,"output":"DP-A","active":true,
         "focused":false,"urgent":false,"focused_window":WINDOW,"layout":"scrolling"},
        {"id":OTHER_WORKSPACE,"name":"1","named":true,"index":1,"output":"DP-B","active":true,
         "focused":true,"urgent":true,"focused_window":OTHER,"layout":"dwindle"}
    ])
}
pub fn windows(title: &str, active: bool) -> Value {
    json!([
        {"id":WINDOW,"title":title,"app_id":"test-app","pid":321,"workspace":WORKSPACE,"output":"DP-A",
         "active":active,"focused":true,"floating":false,"urgent":false,"column":0,"row":1,"future_field":{"any":[1,2]}},
        {"id":OTHER,"title":"remembered elsewhere","app_id":"other-app","pid":-1,"workspace":OTHER_WORKSPACE,"output":"DP-B",
         "active":false,"focused":true,"floating":false,"urgent":true,"column":-1,"row":-1}
    ])
}
