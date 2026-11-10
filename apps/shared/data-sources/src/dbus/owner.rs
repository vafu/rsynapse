use futures_util::{StreamExt, TryStreamExt, future, stream};
use rxrust::prelude::Observable as _;

use super::{
    Bus, Observable, defer, descriptor_error_path, from_stream_result, log_errors, shared_by_key,
};

/// The current unique owner of a well-known service, including initial absence
/// and subsequent owner loss/replacement. Consumers can switch dependent
/// descriptors to that owner without retaining stale properties after restart.
pub fn service_owner(bus: Bus, destination: impl Into<String>) -> Observable<Option<String>> {
    let destination = destination.into();
    let key = format!("{}:{destination}", bus.key());
    shared_by_key("dbus-service-owner", key.clone(), move || {
        let destination = destination.clone();
        let observable = defer(move || {
            let destination = destination.clone();
            let setup = stream::once(async move {
                let connection = bus.connection().await.map_err(|e| e.to_string())?;
                let proxy = zbus::fdo::DBusProxy::new(&connection)
                    .await
                    .map_err(|e| e.to_string())?;
                // Subscribe before the initial query so an owner transition
                // between bootstrap and watching cannot be missed.
                let changes = proxy
                    .receive_name_owner_changed()
                    .await
                    .map_err(|e| e.to_string())?;
                let name = zbus::names::BusName::try_from(destination.as_str())
                    .map_err(|e| e.to_string())?;
                let initial = match proxy.get_name_owner(name).await {
                    Ok(owner) => Some(owner.to_string()),
                    Err(zbus::fdo::Error::NameHasNoOwner(_)) => None,
                    Err(error) => return Err(error.to_string()),
                };
                let updates = changes.filter_map(move |event| {
                    let update = match event.args() {
                        Ok(args) if args.name().as_str() == destination => {
                            Some(Ok(args.new_owner().as_ref().map(ToString::to_string)))
                        }
                        Ok(_) => None,
                        Err(error) => Some(Err(error.to_string())),
                    };
                    future::ready(update)
                });
                Ok(stream::once(future::ready(Ok(initial))).chain(updates))
            })
            .try_flatten();
            from_stream_result(setup)
        })
        .distinct_until_changed()
        .box_it();
        log_errors(
            "dbus-service-owner",
            descriptor_error_path(&key),
            observable,
        )
    })
}
