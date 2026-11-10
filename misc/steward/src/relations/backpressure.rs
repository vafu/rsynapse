use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use zbus::{interface, object_server::SignalContext};

struct Service(Arc<AtomicBool>);
#[interface(name = "org.rsynapse.Locus.Relations1")]
impl Service {
    async fn list(
        &self,
        _relation: &str,
        #[zbus(signal_context)] context: SignalContext<'_>,
    ) -> Vec<RelationRecord> {
        if self.0.swap(false, Ordering::SeqCst) {
            let record = RelationRecord {
                subject: RelationEndpoint::stable_key(locus::keys::WINDOW_ID, "window"),
                relation: "org.rsynapse.unrelated".into(),
                target: RelationEndpoint::stable_key(locus::keys::PROJECT_PATH, "/project"),
                metadata: HashMap::new(),
                created_at_unix_ms: 0,
                updated_at_unix_ms: 0,
            };
            for _ in 0..512 {
                Self::relation_added(&context, record.clone())
                    .await
                    .unwrap();
            }
        }
        vec![]
    }

    #[zbus(signal)]
    async fn relation_added(
        context: &SignalContext<'_>,
        record: RelationRecord,
    ) -> zbus::Result<()>;
}

#[test]
fn relation_burst_cannot_block_its_initial_snapshot() {
    if std::env::var_os("LOCUS_BACKPRESSURE_WORKER").is_none() {
        assert!(
            std::process::Command::new("dbus-run-session")
                .arg("--")
                .arg(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "relations::backpressure::relation_burst_cannot_block_its_initial_snapshot",
                    "--nocapture"
                ])
                .env("LOCUS_BACKPRESSURE_WORKER", "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let _service = zbus::connection::Builder::session()
            .unwrap()
            .serve_at(locus::OBJECT_PATH, Service(Arc::new(AtomicBool::new(true))))
            .unwrap()
            .name(locus::BUS_NAME)
            .unwrap()
            .build()
            .await
            .unwrap();
        let client = LocusClient::connect().await.unwrap();
        let mut stream = records(client, WORKSPACE_PROJECT).into_stream();
        let first = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
            .await
            .expect("Locus signal backpressure blocked List reply")
            .unwrap()
            .unwrap();
        assert!(first.is_empty());
    });
}
