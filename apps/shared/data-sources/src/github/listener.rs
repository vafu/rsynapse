use std::path::Path;

use futures_util::{StreamExt, future};
use zbus::{Connection, proxy::CacheProperties, zvariant::OwnedObjectPath};

use crate::{
    Observable,
    dbus::{Bus, ObjectDescriptor, PropertyDescriptor, property},
    rx::Observable as _,
};

use super::{BUS_NAME, PR_PATH};

#[zbus::proxy(interface = "io.github.vfuchedzhy.GithubPr.Listeners")]
trait Factory {
    fn create(&self, labels: Vec<String>) -> zbus::Result<OwnedObjectPath>;
}

#[zbus::proxy(interface = "io.github.vfuchedzhy.GithubPr.Listener")]
trait Listener {
    #[zbus(no_reply)]
    fn close(&self) -> zbus::Result<()>;
}

/// Sorted PR URLs carrying **all** the exact labels; no labels matches every PR.
///
/// `None` means unavailable/loading; `Some(vec![])` is a loaded empty result.
/// Equivalent label sets share one remote listener until the last subscriber
/// drops. A replacement service owner always starts with `None` and a new lease.
pub fn listener(mut labels: Vec<String>) -> Observable<Option<Vec<String>>> {
    labels.sort();
    labels.dedup();
    // JSON is an unambiguous key even for labels containing separators/escapes.
    let key = serde_json::to_string(&labels).expect("strings serialize");
    crate::shared_by_key("github-listener", key, move || {
        let labels = labels.clone();
        crate::switch_map(
            crate::dbus::service_owner(Bus::Session, BUS_NAME),
            move |owner| {
                let Some(owner) = owner else {
                    return crate::once(None);
                };
                let labels = labels.clone();
                crate::from_task(move |sender| {
                    let owner = owner.clone();
                    let labels = labels.clone();
                    async move {
                        if let Err(error) = watch(owner, labels, &sender).await {
                            crate::support::record_source_error(
                                "github-listener",
                                Path::new(PR_PATH),
                                &error.to_string(),
                            );
                            let _ = sender.send(Ok(None)).await;
                        }
                        // Setup failures park until ownership changes, rather than
                        // repeatedly creating listeners against an incompatible owner.
                        future::pending::<()>().await;
                    }
                })
                .start_with(vec![None])
                .box_it()
            },
        )
        .distinct_until_changed()
        .box_it()
    })
}

async fn watch(
    owner: String,
    labels: Vec<String>,
    sender: &async_channel::Sender<Result<Option<Vec<String>>, String>>,
) -> zbus::Result<()> {
    // Never use the process-cached connection: this connection's disappearance
    // is also the cleanup mechanism if cancellation interrupts Create's reply.
    let connection = Connection::session().await?;
    let mut lease = Lease {
        connection: Some(connection),
        owner,
        path: None,
    };
    let connection = lease.connection.as_ref().expect("live lease");
    let factory = FactoryProxy::builder(connection)
        .destination(lease.owner.as_str())?
        .path(PR_PATH)?
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    lease.path = Some(factory.create(labels).await?);
    let object = ObjectDescriptor::parse(
        Bus::Session,
        &lease.owner,
        lease.path.as_ref().expect("created listener").as_str(),
        "io.github.vfuchedzhy.GithubPr.Listener",
    )
    .expect("valid unique owner and factory object path");
    // The generic property bridge installs a raw PropertiesChanged watch before
    // Get, then drains queued updates. Unlike zbus 4's proxy cache bootstrap,
    // it retains changes emitted while a captured initial snapshot is in flight.
    // Valued changes are decoded directly; only invalidations cause another Get.
    // The lease stays here for the entire property subscription, even though
    // property reads use the generic bridge's process-shared connection.
    let mut changes =
        property::<Vec<String>>(PropertyDescriptor::new(object, "PullRequests")).into_stream();
    while let Some(change) = changes.next().await {
        if sender.send(change).await.is_err() {
            return Ok(());
        }
    }
    Ok(())
}

/// Owns the creator connection even while Create is in flight. There is no
/// detached task waiting for a Create reply after cancellation.
struct Lease {
    connection: Option<Connection>,
    owner: String,
    path: Option<OwnedObjectPath>,
}

impl Drop for Lease {
    fn drop(&mut self) {
        let Some(connection) = self.connection.take() else {
            return;
        };
        let owner = self.owner.clone();
        let path = self.path.take();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                if let Some(path) = path {
                    let proxy = ListenerProxy::builder(&connection)
                        .destination(owner)
                        .and_then(|builder| builder.path(path));
                    if let Ok(builder) = proxy {
                        if let Ok(proxy) =
                            builder.cache_properties(CacheProperties::No).build().await
                        {
                            // NoReplyExpected still sends Close on the creator
                            // connection, but cannot strand a task on a reply.
                            let _ = proxy.close().await;
                        }
                    }
                }
                let _ = connection.close().await;
            });
        }
        // During runtime/process shutdown, dropping the connection suffices.
    }
}
