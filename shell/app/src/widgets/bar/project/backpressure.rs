use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use zbus::{interface, object_server::SignalContext};

struct Relations(Arc<AtomicBool>);
#[interface(name = "org.rsynapse.Locus.Relations1")]
impl Relations {
    async fn list(
        &self,
        _relation: &str,
        #[zbus(signal_context)] context: SignalContext<'_>,
    ) -> Vec<RelationRecord> {
        let record = RelationRecord {
            subject: workspace_subject("selected".into()),
            relation: WORKSPACE_PROJECT_RELATION.into(),
            target: RelationEndpoint::stable_key(keys::PROJECT_PATH, "/project"),
            metadata: HashMap::new(),
            created_at_unix_ms: 0,
            updated_at_unix_ms: 0,
        };
        if self.0.swap(false, Ordering::SeqCst) {
            let unrelated = RelationRecord {
                subject: workspace_subject("other".into()),
                ..record.clone()
            };
            for _ in 0..512 {
                Self::relation_added(&context, unrelated.clone())
                    .await
                    .unwrap();
            }
        }
        vec![record]
    }

    #[zbus(signal)]
    async fn relation_added(
        context: &SignalContext<'_>,
        record: RelationRecord,
    ) -> zbus::Result<()>;
}

#[test]
fn locus_burst_cannot_freeze_project_assignment_display() {
    if std::env::var_os("SHELL_PROJECT_BACKPRESSURE_WORKER").is_none() {
        assert!(std::process::Command::new("dbus-run-session").arg("--")
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", "widgets::bar::project::backpressure::locus_burst_cannot_freeze_project_assignment_display", "--nocapture"])
            .env("SHELL_PROJECT_BACKPRESSURE_WORKER", "1").status().unwrap().success());
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let _service = zbus::connection::Builder::session()
            .unwrap()
            .serve_at(
                locus::OBJECT_PATH,
                Relations(Arc::new(AtomicBool::new(true))),
            )
            .unwrap()
            .name(locus::BUS_NAME)
            .unwrap()
            .build()
            .await
            .unwrap();
        let (sender, receiver) = async_channel::bounded(1);
        let watcher = tokio::spawn(run_locus_workspace_project(
            sender,
            workspace_subject("selected".into()),
        ));
        let first = tokio::time::timeout(std::time::Duration::from_secs(5), receiver.recv())
            .await
            .expect("Locus signal burst blocked the shell's project List reply")
            .unwrap()
            .unwrap();
        assert!(first.has_project);
        assert_eq!(first.path.as_deref(), Some("/project"));
        assert!(!watcher.is_finished());
        watcher.abort();
    });
}
