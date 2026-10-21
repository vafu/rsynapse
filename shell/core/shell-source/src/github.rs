//! UI-free reactive client for the `github-dbus` service.
//!
//! The service publishes open + recently-merged PRs as JSON-string
//! properties and notification-thread updates as a JSON-string signal; this
//! module exposes them as typed observables over the shared D-Bus source
//! primitives.

use crate::{
    Observable,
    dbus::{
        Bus, ObjectDescriptor, PropertyDescriptor, SignalDescriptor, property, service_owner,
        signal,
    },
    rx::Observable as _,
};
pub use github_dbus_api::{
    NotificationEvent, PrOverview, PrStatus, PrSummary, PullRequest, PullRequestActivity,
};

pub const BUS_NAME: &str = "io.github.vfuchedzhy.GithubPr";
pub const PR_PATH: &str = "/io/github/vfuchedzhy/GithubPr";
pub const NOTIFICATIONS_PATH: &str = "/io/github/vfuchedzhy/GithubNotifications";
const PR_INTERFACE: &str = "io.github.vfuchedzhy.GithubPr";
const NOTIFICATIONS_INTERFACE: &str = "io.github.vfuchedzhy.GithubNotifications";

fn pr_object() -> ObjectDescriptor {
    ObjectDescriptor::parse(Bus::Session, BUS_NAME, PR_PATH, PR_INTERFACE)
        .expect("static PR descriptor is valid")
}

fn notifications_object() -> ObjectDescriptor {
    ObjectDescriptor::parse(
        Bus::Session,
        BUS_NAME,
        NOTIFICATIONS_PATH,
        NOTIFICATIONS_INTERFACE,
    )
    .expect("static notifications descriptor is valid")
}

/// Parses a JSON-string property; `None` while absent, invalid, or undecodable.
fn json_property<T>(descriptor: PropertyDescriptor) -> Observable<Option<T>>
where
    T: serde::de::DeserializeOwned + Clone + PartialEq + Send + 'static,
{
    crate::switch_map(
        service_owner(
            descriptor.object.bus,
            descriptor.object.destination.to_string(),
        ),
        move |owner| {
            let Some(owner) = owner else {
                return crate::once(None);
            };
            let mut descriptor = descriptor.clone();
            descriptor.object.destination = owner
                .try_into()
                .expect("D-Bus returned a valid unique owner");
            property::<String>(descriptor)
                .map(|value| value.and_then(|json| serde_json::from_str(&json).ok()))
                .box_it()
        },
    )
    .distinct_until_changed()
    .box_it()
}

/// Open + recently-merged PRs with derived statuses.
pub fn pull_requests() -> Observable<Option<Vec<PullRequest>>> {
    crate::shared_by_key("github-pull-requests", BUS_NAME, || {
        json_property(PropertyDescriptor::new(pr_object(), "PullRequests"))
    })
}

/// Current PR revisions with persistent unseen markers, as one coherent list.
pub fn activity() -> Observable<Option<Vec<PullRequestActivity>>> {
    crate::shared_by_key("github-pr-activity", BUS_NAME, || {
        json_property::<Option<Vec<PullRequestActivity>>>(PropertyDescriptor::new(
            pr_object(),
            "PullRequestActivity",
        ))
        .map(Option::flatten)
        .distinct_until_changed()
        .box_it()
    })
}

/// Per-status PR counts.
pub fn summary() -> Observable<Option<PrSummary>> {
    json_property(PropertyDescriptor::new(pr_object(), "Summary"))
}

/// Explicit panel rollups, including open PRs excluding recently merged PRs.
pub fn overview() -> Observable<Option<PrOverview>> {
    json_property(PropertyDescriptor::new(pr_object(), "Overview"))
}

/// Notification-thread updates (new comments, mentions, review requests, ...).
pub fn notifications() -> Observable<NotificationEvent> {
    signal::<String>(SignalDescriptor::new(
        notifications_object(),
        "NotificationReceived",
    ))
    .filter_map(|json| serde_json::from_str(&json).ok())
    .box_it()
}

/// Asks the daemon to refresh immediately.
pub async fn refresh() -> Result<(), String> {
    let conn = zbus::Connection::session()
        .await
        .map_err(|e| e.to_string())?;
    let proxy = zbus::Proxy::new(&conn, BUS_NAME, PR_PATH, PR_INTERFACE)
        .await
        .map_err(|e| e.to_string())?;
    proxy
        .call::<_, _, ()>("Refresh", &())
        .await
        .map_err(|e| e.to_string())
}
