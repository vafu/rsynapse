use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use futures_util::StreamExt;
use shell_source::github;
use tokio::sync::{Semaphore, mpsc};
use zbus::{Connection, fdo, interface, object_server::SignalContext, zvariant::OwnedObjectPath};

pub const INTERFACE: &str = "io.github.vfuchedzhy.GithubPr.Listener";

#[derive(Clone, Debug)]
pub enum Event {
    Created {
        path: String,
        creator: String,
        labels: Vec<String>,
    },
    Closed(String),
    Reaped(String),
}

pub struct State {
    pub inventory: Vec<(String, Vec<String>)>,
    pub events: mpsc::UnboundedSender<Event>,
    pub creates: AtomicUsize,
    pub reads: AtomicUsize,
    pub closes: AtomicUsize,
    pub active: Mutex<HashMap<String, String>>,
    pub create_gate: Option<Arc<Semaphore>>,
    pub read_gate: Option<Arc<Semaphore>>,
    pub read_started: Semaphore,
}

impl State {
    pub fn new(create_gate: bool, read_gate: bool) -> (Arc<Self>, mpsc::UnboundedReceiver<Event>) {
        let (events, receiver) = mpsc::unbounded_channel();
        (
            Arc::new(Self {
                inventory: vec![
                    (
                        "https://test/pr/1".into(),
                        vec!["project".into(), "ci".into()],
                    ),
                    ("https://test/pr/2".into(), vec!["project".into()]),
                    ("https://test/pr/3".into(), vec!["other".into()]),
                ],
                events,
                creates: AtomicUsize::new(0),
                reads: AtomicUsize::new(0),
                closes: AtomicUsize::new(0),
                active: Mutex::new(HashMap::new()),
                create_gate: create_gate.then(|| Arc::new(Semaphore::new(0))),
                read_gate: read_gate.then(|| Arc::new(Semaphore::new(0))),
                read_started: Semaphore::new(0),
            }),
            receiver,
        )
    }
}

struct Factory(Arc<State>);

#[interface(name = "io.github.vfuchedzhy.GithubPr.Listeners")]
impl Factory {
    async fn create(
        &self,
        labels: Vec<String>,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<OwnedObjectPath> {
        let creator = header.sender().unwrap().to_string();
        let id = self.0.creates.fetch_add(1, Ordering::SeqCst);
        let path = format!("{}/listeners/{id}", github::PR_PATH);
        let urls = self
            .0
            .inventory
            .iter()
            .filter(|(_, pr_labels)| labels.iter().all(|label| pr_labels.contains(label)))
            .map(|(url, _)| url.clone())
            .collect();
        connection
            .object_server()
            .at(
                path.clone(),
                Listener {
                    state: self.0.clone(),
                    labels: labels.clone(),
                    urls: Mutex::new(urls),
                    creator: creator.clone(),
                },
            )
            .await
            .unwrap();
        self.0
            .active
            .lock()
            .unwrap()
            .insert(path.clone(), creator.clone());

        // Install the unique-name watch before announcing creation. This fake
        // deliberately does not depend on any backend implementation crate.
        let dbus = fdo::DBusProxy::new(connection).await.unwrap();
        let mut changes = dbus.receive_name_owner_changed().await.unwrap();
        let conn = connection.clone();
        let state = self.0.clone();
        let watch_path = path.clone();
        let watch_creator = creator.clone();
        tokio::spawn(async move {
            while let Some(change) = changes.next().await {
                let args = change.args().unwrap();
                if args.name().as_str() == watch_creator && args.new_owner().is_none() {
                    let _ = conn
                        .object_server()
                        .remove::<Listener, _>(watch_path.as_str())
                        .await;
                    state.active.lock().unwrap().remove(&watch_path);
                    let _ = state.events.send(Event::Reaped(watch_path));
                    return;
                }
            }
        });
        self.0
            .events
            .send(Event::Created {
                path: path.clone(),
                creator,
                labels,
            })
            .unwrap();
        if let Some(gate) = &self.0.create_gate {
            gate.acquire().await.unwrap().forget();
        }
        Ok(path.try_into().unwrap())
    }
}

struct Listener {
    state: Arc<State>,
    labels: Vec<String>,
    urls: Mutex<Vec<String>>,
    creator: String,
}

#[interface(name = "io.github.vfuchedzhy.GithubPr.Listener")]
impl Listener {
    #[zbus(property)]
    fn labels(&self) -> Vec<String> {
        self.labels.clone()
    }

    #[zbus(property)]
    async fn pull_requests(&self) -> Vec<String> {
        self.state.reads.fetch_add(1, Ordering::SeqCst);
        // Capture the snapshot before the test changes membership, exercising
        // signals that race a bootstrap reply.
        let snapshot = self.urls.lock().unwrap().clone();
        self.state.read_started.add_permits(1);
        if let Some(gate) = &self.state.read_gate {
            gate.acquire().await.unwrap().forget();
        }
        snapshot
    }

    async fn close(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_context)] context: SignalContext<'_>,
    ) -> fdo::Result<()> {
        assert_eq!(
            header.sender().unwrap().as_str(),
            self.creator,
            "Close must use creator connection"
        );
        let path = context.path().to_string();
        self.state.closes.fetch_add(1, Ordering::SeqCst);
        self.state.active.lock().unwrap().remove(&path);
        // Disconnect cleanup may race Close after the no-reply message arrived.
        let _ = context
            .connection()
            .object_server()
            .remove::<Self, _>(context.path())
            .await;
        let _ = self.state.events.send(Event::Closed(path));
        Ok(())
    }
}

struct Json;

#[interface(name = "io.github.vfuchedzhy.GithubPr")]
impl Json {
    #[zbus(property)]
    fn overview(&self) -> String {
        serde_json::to_string(&github::PrOverview {
            open: 7,
            ..Default::default()
        })
        .unwrap()
    }
}

pub async fn serve(state: Arc<State>, claim: bool) -> Connection {
    let connection = zbus::connection::Builder::session()
        .unwrap()
        .serve_at(github::PR_PATH, Factory(state))
        .unwrap()
        .serve_at(github::PR_PATH, Json)
        .unwrap()
        .build()
        .await
        .unwrap();
    if claim {
        connection
            .request_name_with_flags(
                github::BUS_NAME,
                fdo::RequestNameFlags::AllowReplacement | fdo::RequestNameFlags::DoNotQueue,
            )
            .await
            .unwrap();
    }
    connection
}

pub async fn update(connection: &Connection, path: &str, urls: Vec<String>) {
    let interface = connection
        .object_server()
        .interface::<_, Listener>(path)
        .await
        .unwrap();
    *interface.get().await.urls.lock().unwrap() = urls.clone();
    // Emit the wire schema directly: the property getter must not run merely
    // to construct this test signal, so read-count assertions are meaningful.
    emit(connection, path, urls).await;
}

pub async fn emit(connection: &Connection, path: &str, urls: Vec<String>) {
    let changed: HashMap<&str, zbus::zvariant::Value<'_>> =
        HashMap::from([("PullRequests", zbus::zvariant::Value::from(urls))]);
    connection
        .emit_signal(
            None::<&str>,
            path,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &(INTERFACE, changed, Vec::<String>::new()),
        )
        .await
        .unwrap();
}
