use std::{collections::HashMap, process::Command, time::Duration};

use futures_util::{Stream, StreamExt};
use shell_core::source::rx::Observable as _;
use tokio::sync::{RwLock, Semaphore};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use super::super::{
    AGENT_DBUS_BUS, AGENT_DBUS_ROOT_PATH, AGENT_SESSION_INTERFACE, actual::raw_agent_for_window,
};
use super::{Agent, State};

const A: &str = "/io/github/AgentDBus/sessions/a";
const B: &str = "/io/github/AgentDBus/sessions/b";
const LEGACY: &str = "/io/github/AgentDBus/sessions/legacy";
type Objects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

struct Manager;

#[zbus::interface(name = "org.freedesktop.DBus.ObjectManager")]
impl Manager {
    fn get_managed_objects(&self) -> Objects {
        [(A, "window-a"), (B, "window-b"), (LEGACY, "legacy")]
            .into_iter()
            .map(|(path, window)| {
                (
                    OwnedObjectPath::try_from(path).unwrap(),
                    HashMap::from([(
                        AGENT_SESSION_INTERFACE.to_owned(),
                        HashMap::from([(
                            "WindowId".to_owned(),
                            OwnedValue::try_from(zbus::zvariant::Value::from(window)).unwrap(),
                        )]),
                    )]),
                )
            })
            .collect()
    }
}

struct Session {
    name: &'static str,
    percent: RwLock<Option<f64>>,
    ready: Semaphore,
    state: RwLock<String>,
}

impl Session {
    fn new(name: &'static str, percent: Option<f64>, ready: usize) -> Self {
        Self {
            name,
            percent: RwLock::new(percent),
            ready: Semaphore::new(ready),
            state: RwLock::new("thinking".into()),
        }
    }
}

#[zbus::interface(name = "io.github.AgentDBus1.Session")]
impl Session {
    #[zbus(property)]
    fn agent_name(&self) -> &str {
        self.name
    }
    #[zbus(property)]
    fn agent_nickname(&self) -> &str {
        ""
    }
    #[zbus(property)]
    fn agent_role(&self) -> &str {
        ""
    }
    #[zbus(property)]
    async fn state(&self) -> String {
        self.state.read().await.clone()
    }
    #[zbus(property)]
    fn requires_attention(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn cwd(&self) -> &str {
        "/tmp"
    }
    #[zbus(property)]
    fn session_title(&self) -> &str {
        self.name
    }
    #[zbus(property)]
    fn session_id(&self) -> &str {
        self.name
    }
    #[zbus(property)]
    async fn context_pct(&self) -> zbus::fdo::Result<f64> {
        let _permit = self.ready.acquire().await.unwrap();
        self.percent
            .read()
            .await
            .ok_or_else(|| zbus::fdo::Error::UnknownProperty("ContextPct".into()))
    }
}

async fn expect(
    stream: &mut (impl Stream<Item = Result<Option<Agent>, String>> + Unpin),
    name: &str,
    percent: Option<u8>,
    state: State,
) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let Some(agent) = stream.next().await.unwrap().unwrap() else {
                continue;
            };
            assert_eq!(agent.name, name, "a different window's session leaked");
            if agent.context_pct == percent && agent.state == state {
                break;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{name} did not emit {percent:?} in {state:?}"));
}

async fn publish(server: &zbus::Connection, path: &str, percent: f64) {
    let handle = server
        .object_server()
        .interface::<_, Session>(path)
        .await
        .unwrap();
    *handle.get().await.percent.write().await = Some(percent);
    handle
        .get()
        .await
        .context_pct_changed(handle.signal_context())
        .await
        .unwrap();
}

#[test]
fn context_tracks_each_session_without_blocking_agent_state() {
    if std::env::var_os("SHELL_AGENT_CONTEXT_TEST_CHILD").is_none() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("bus.conf");
        std::fs::write(&config, r#"<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*" eavesdrop="true"/><allow eavesdrop="true"/><allow own="*"/></policy></busconfig>"#).unwrap();
        let mut command = Command::new("dbus-run-session");
        command
            .arg(format!("--config-file={}", config.display()))
            .arg("--");
        if let Ok(runner) = std::env::var("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER") {
            command.args(shell_words::split(&runner).unwrap());
        }
        let name = concat!(
            module_path!(),
            "::context_tracks_each_session_without_blocking_agent_state"
        );
        assert!(
            command
                .arg(std::env::args_os().next().unwrap())
                .args(["--exact", name.split_once("::").unwrap().1, "--nocapture"])
                .env("SHELL_AGENT_CONTEXT_TEST_CHILD", "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let setup = zbus::connection::Builder::session()
            .unwrap()
            .serve_at(AGENT_DBUS_ROOT_PATH, Manager)
            .unwrap()
            .serve_at(A, Session::new("a", Some(65.4), 1))
            .unwrap()
            .serve_at(B, Session::new("b", Some(27.8), 1))
            .unwrap()
            .serve_at(LEGACY, Session::new("legacy", None, 1))
            .unwrap()
            .name(AGENT_DBUS_BUS)
            .unwrap()
            .build();
        let server = tokio::time::timeout(Duration::from_secs(5), setup)
            .await
            .expect("fake AgentDBus failed to start")
            .unwrap();
        let first = server
            .object_server()
            .interface::<_, Session>(A)
            .await
            .unwrap();
        // Registration may read properties for InterfacesAdded; block only the
        // consumer's later ContextPct read, not the fake service's bootstrap.
        first.get().await.ready.forget_permits(1);
        let mut a = raw_agent_for_window("window-a".into()).into_stream();
        let mut b = raw_agent_for_window("window-b".into()).into_stream();
        let mut legacy = raw_agent_for_window("legacy".into()).into_stream();

        // A blocked context getter cannot prevent an agent icon/lifecycle update.
        expect(&mut a, "a", None, State::Thinking).await;
        expect(&mut b, "b", Some(28), State::Thinking).await;
        expect(&mut legacy, "legacy", None, State::Thinking).await;
        let old = server
            .object_server()
            .interface::<_, Session>(LEGACY)
            .await
            .unwrap();
        *old.get().await.state.write().await = "compacting".into();
        old.get()
            .await
            .state_changed(old.signal_context())
            .await
            .unwrap();
        expect(&mut legacy, "legacy", None, State::Compacting).await;

        first.get().await.ready.add_permits(1);
        expect(&mut a, "a", Some(65), State::Thinking).await;
        publish(&server, A, 90.5).await;
        expect(&mut a, "a", Some(91), State::Thinking).await;
        publish(&server, B, 12.1).await;
        expect(&mut b, "b", Some(12), State::Thinking).await;
        for (input, expected) in [
            (f64::NAN, None),
            (150.0, Some(100)),
            (-1.0, Some(0)),
            (42.2, Some(42)),
        ] {
            publish(&server, A, input).await;
            expect(&mut a, "a", expected, State::Thinking).await;
        }

        server
            .emit_signal(
                None::<&str>,
                AGENT_DBUS_ROOT_PATH,
                "org.freedesktop.DBus.ObjectManager",
                "InterfacesRemoved",
                &(
                    OwnedObjectPath::try_from(A).unwrap(),
                    vec![AGENT_SESSION_INTERFACE],
                ),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while a.next().await.unwrap().unwrap().is_some() {}
        })
        .await
        .expect("removed session retained its context meter");
        drop((first, old, a, b, legacy, server));
    });
}
