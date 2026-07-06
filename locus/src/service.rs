use std::{collections::HashMap, sync::Arc};

use tokio::sync::Mutex;
use tracing::info;
use zbus::{connection::Builder, fdo, interface, object_server::SignalContext};

use locus::{BUS_NAME, OBJECT_PATH, RelationEndpoint, RelationRecord};

use crate::store::{RelationStore, SetOutcome, default_store_path};

pub async fn run() -> anyhow::Result<()> {
    let store = RelationStore::open(default_store_path())?;
    let service = RelationsService::new(store);

    let _connection = Builder::session()?
        .serve_at(OBJECT_PATH, service)?
        .name(BUS_NAME)?
        .build()
        .await?;

    info!("owning {BUS_NAME} at {OBJECT_PATH}");
    tokio::signal::ctrl_c().await?;
    Ok(())
}

pub struct RelationsService {
    store: Arc<Mutex<RelationStore>>,
}

impl RelationsService {
    pub fn new(store: RelationStore) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
        }
    }
}

#[interface(name = "org.rsynapse.Locus.Relations1")]
impl RelationsService {
    #[zbus(property)]
    async fn record_count(&self) -> u64 {
        self.store.lock().await.len().try_into().unwrap_or(u64::MAX)
    }

    #[zbus(property)]
    async fn relations(&self) -> Vec<String> {
        self.store.lock().await.relations()
    }

    async fn set(
        &self,
        subject: RelationEndpoint,
        relation: String,
        target: RelationEndpoint,
        metadata: HashMap<String, String>,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
    ) -> fdo::Result<RelationRecord> {
        let outcome = self
            .store
            .lock()
            .await
            .set(subject, relation, target, metadata)
            .map_err(fdo_error)?;
        let record = outcome.record.clone();
        self.emit_store_properties(&ctxt).await?;
        Self::emit_set_outcome(&ctxt, outcome).await?;
        Ok(record)
    }

    async fn set_one(
        &self,
        subject: RelationEndpoint,
        relation: String,
        target: RelationEndpoint,
        metadata: HashMap<String, String>,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
    ) -> fdo::Result<RelationRecord> {
        let outcome = self
            .store
            .lock()
            .await
            .set_one(subject, relation, target, metadata)
            .map_err(fdo_error)?;
        let record = outcome.set.record.clone();
        self.emit_store_properties(&ctxt).await?;
        for removed in outcome.removed {
            Self::relation_removed(&ctxt, removed).await?;
        }
        Self::emit_set_outcome(&ctxt, outcome.set).await?;
        Ok(record)
    }

    async fn unset(
        &self,
        subject: RelationEndpoint,
        relation: String,
        target: RelationEndpoint,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
    ) -> fdo::Result<bool> {
        let removed = self
            .store
            .lock()
            .await
            .unset(&subject, &relation, &target)
            .map_err(fdo_error)?;
        if let Some(record) = removed {
            self.emit_store_properties(&ctxt).await?;
            Self::relation_removed(&ctxt, record).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn clear(
        &self,
        subject: RelationEndpoint,
        relation: String,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
    ) -> fdo::Result<u32> {
        let removed = self
            .store
            .lock()
            .await
            .clear(&subject, &relation)
            .map_err(fdo_error)?;
        let count = removed.len().try_into().unwrap_or(u32::MAX);
        if count > 0 {
            self.emit_store_properties(&ctxt).await?;
            for record in removed {
                Self::relation_removed(&ctxt, record).await?;
            }
            Self::relation_cleared(&ctxt, subject, relation, count).await?;
        }
        Ok(count)
    }

    async fn targets(&self, subject: RelationEndpoint, relation: String) -> Vec<RelationEndpoint> {
        self.store.lock().await.targets(&subject, &relation)
    }

    async fn subjects(&self, relation: String, target: RelationEndpoint) -> Vec<RelationEndpoint> {
        self.store.lock().await.subjects(&relation, &target)
    }

    async fn list(&self, relation: String) -> Vec<RelationRecord> {
        self.store.lock().await.list(&relation)
    }

    #[zbus(signal)]
    async fn relation_added(ctxt: &SignalContext<'_>, record: RelationRecord) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn relation_updated(ctxt: &SignalContext<'_>, record: RelationRecord)
    -> zbus::Result<()>;

    #[zbus(signal)]
    async fn relation_removed(ctxt: &SignalContext<'_>, record: RelationRecord)
    -> zbus::Result<()>;

    #[zbus(signal)]
    async fn relation_cleared(
        ctxt: &SignalContext<'_>,
        subject: RelationEndpoint,
        relation: String,
        removed_count: u32,
    ) -> zbus::Result<()>;
}

impl RelationsService {
    async fn emit_store_properties(&self, ctxt: &SignalContext<'_>) -> zbus::Result<()> {
        self.record_count_changed(ctxt).await?;
        self.relations_changed(ctxt).await
    }

    async fn emit_set_outcome(ctxt: &SignalContext<'_>, outcome: SetOutcome) -> zbus::Result<()> {
        if outcome.created {
            Self::relation_added(ctxt, outcome.record).await
        } else {
            Self::relation_updated(ctxt, outcome.record).await
        }
    }
}

fn fdo_error(error: std::io::Error) -> fdo::Error {
    match error.kind() {
        std::io::ErrorKind::InvalidInput => fdo::Error::InvalidArgs(error.to_string()),
        _ => fdo::Error::Failed(error.to_string()),
    }
}
