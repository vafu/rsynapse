//! AgentDBus provider usage objects and their Locus icon preferences.
use std::collections::HashMap;

use locus::{RelationEndpoint, RelationRecord};

use crate::{Observable, rx::Observable as _, watch_relation};
use zbus::zvariant::OwnedValue;

const BUS_NAME: &str = "io.github.AgentDBus";
const ROOT_PATH: &str = "/io/github/AgentDBus";
const PROVIDERS_PATH: &str = "/io/github/AgentDBus/providers";
const PROVIDERS_INTERFACE: &str = "io.github.AgentDBus1.Providers";
const PROVIDER_INTERFACE: &str = "io.github.AgentDBus1.Provider";
pub const ICON_OVERRIDE_RELATION: &str = "org.rsynapse.provider.icon-override";
const PROVIDER_ID_KIND: &str = "org.rsynapse.provider.id";
const ICON_GLYPH_KIND: &str = "org.rsynapse.icon.glyph";
const PICKER_INPUT_METADATA: &str = "pick-icon-input";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProviderUsage {
    pub id: String,
    pub path: String,
    pub name: String,
    pub kind: String,
    pub enabled: bool,
    pub state: String,
    pub label: String,
    pub has_limit: bool,
    pub limit_used_pct: Option<u8>,
    pub limit_usd: f64,
    pub limit_remaining_usd: f64,
    pub limit_reset: String,
    pub include_byok_in_limit: bool,
    pub usage_usd: f64,
    pub usage_daily_usd: f64,
    pub usage_weekly_usd: f64,
    pub usage_monthly_usd: f64,
    pub byok_usage_usd: f64,
    pub byok_usage_daily_usd: f64,
    pub byok_usage_weekly_usd: f64,
    pub byok_usage_monthly_usd: f64,
    pub is_free_tier: bool,
    pub free_model_daily_used: u64,
    pub free_model_daily_limit: u64,
    pub free_model_daily_remaining: u64,
    pub last_attempt_unix: u64,
    pub last_success_unix: u64,
    pub last_http_status: u16,
    pub refresh_error: String,
    pub stale: bool,
    pub revision: u64,
}

impl ProviderUsage {
    pub fn bar_percent(&self) -> Option<u8> {
        self.has_limit.then_some(self.limit_used_pct).flatten()
    }

    pub fn tooltip(&self) -> String {
        let percent = self
            .bar_percent()
            .map(|percent| format!(" · {percent}% used"))
            .unwrap_or_default();
        format!(
            "{}\n{} · {}{}\nspent {}",
            self.name,
            self.state,
            if self.stale { "stale" } else { "live" },
            percent,
            money(self.usage_usd),
        )
    }
}

pub fn providers() -> Observable<Vec<ProviderUsage>> {
    use futures_util::StreamExt;

    let key = "rsynapse.provider-usage";
    crate::shared_by_key(key, key, || {
        crate::from_task(|sender| async move {
            let connection = match zbus::Connection::session().await {
                Ok(connection) => connection,
                Err(error) => {
                    let _ = sender
                        .send(Err(format!("connect session bus: {error}")))
                        .await;
                    return;
                }
            };
            let object_manager = zbus::Proxy::new(
                &connection,
                BUS_NAME,
                ROOT_PATH,
                "org.freedesktop.DBus.ObjectManager",
            )
            .await;
            let object_manager = match object_manager {
                Ok(proxy) => proxy,
                Err(error) => {
                    let _ = sender
                        .send(Err(format!("connect provider manager: {error}")))
                        .await;
                    return;
                }
            };
            let mut membership = match object_manager.receive_signal("InterfacesAdded").await {
                Ok(stream) => Box::pin(stream),
                Err(error) => {
                    let _ = sender
                        .send(Err(format!("subscribe provider objects: {error}")))
                        .await;
                    return;
                }
            };
            let mut removed = match object_manager.receive_signal("InterfacesRemoved").await {
                Ok(stream) => Box::pin(stream),
                Err(error) => {
                    let _ = sender
                        .send(Err(format!("subscribe provider objects: {error}")))
                        .await;
                    return;
                }
            };
            let mut provider_paths: Vec<String> = Vec::new();
            let mut properties: std::pin::Pin<
                Box<dyn futures_util::Stream<Item = zbus::Message> + Send>,
            > = Box::pin(futures_util::stream::empty());
            loop {
                let snapshot = provider_snapshot(&connection).await;
                let paths = match &snapshot {
                    Ok(providers) => providers
                        .iter()
                        .map(|provider| provider.path.clone())
                        .collect::<Vec<_>>(),
                    Err(_) => Vec::new(),
                };
                if paths != provider_paths {
                    provider_paths = paths;
                    properties = subscribe_provider_properties(&connection, &provider_paths)
                        .await
                        .unwrap_or_else(|_| Box::pin(futures_util::stream::empty()));
                }
                if sender.send(snapshot).await.is_err() {
                    return;
                }
                tokio::select! {
                    _ = membership.next() => {}
                    _ = removed.next() => {}
                    _ = properties.next() => {}
                }
            }
        })
        .distinct_until_changed()
        .box_it()
    })
}

async fn subscribe_provider_properties(
    connection: &zbus::Connection,
    paths: &[String],
) -> Result<std::pin::Pin<Box<dyn futures_util::Stream<Item = zbus::Message> + Send>>, String> {
    let mut streams = Vec::new();
    for path in paths {
        let proxy = zbus::Proxy::new(
            connection,
            BUS_NAME,
            path.as_str(),
            "org.freedesktop.DBus.Properties",
        )
        .await
        .map_err(|error| format!("connect provider properties: {error}"))?;
        if let Ok(stream) = proxy.receive_signal("PropertiesChanged").await {
            streams.push(Box::pin(stream)
                as std::pin::Pin<
                    Box<dyn futures_util::Stream<Item = zbus::Message> + Send>,
                >);
        }
    }
    Ok(Box::pin(futures_util::stream::select_all(streams)))
}

async fn provider_snapshot(connection: &zbus::Connection) -> Result<Vec<ProviderUsage>, String> {
    let proxy = zbus::Proxy::new(
        connection,
        BUS_NAME,
        ROOT_PATH,
        "org.freedesktop.DBus.ObjectManager",
    )
    .await
    .map_err(|error| format!("connect provider manager: {error}"))?;
    let objects: HashMap<
        zbus::zvariant::OwnedObjectPath,
        HashMap<zbus::names::OwnedInterfaceName, HashMap<String, OwnedValue>>,
    > = proxy
        .call("GetManagedObjects", &())
        .await
        .map_err(|error| format!("read provider objects: {error}"))?;
    let mut providers: Vec<ProviderUsage> = objects
        .into_iter()
        .filter_map(|(path, interfaces)| {
            interfaces
                .into_iter()
                .find(|(interface, _)| interface.as_str() == PROVIDER_INTERFACE)
                .map(|(_, properties)| (path, properties))
        })
        .filter_map(|(path, properties)| provider_from_properties(path.as_str(), &properties))
        .collect();
    providers.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(providers)
}

fn provider_from_properties(
    path: &str,
    properties: &HashMap<String, OwnedValue>,
) -> Option<ProviderUsage> {
    fn value<T: TryFrom<OwnedValue>>(
        properties: &HashMap<String, OwnedValue>,
        name: &str,
    ) -> Option<T> {
        T::try_from(properties.get(name)?.try_clone().ok()?).ok()
    }
    let id: String = value(properties, "Id")?;
    let limit_usd: f64 = value(properties, "LimitUsd").unwrap_or_default();
    let remaining: f64 = value(properties, "LimitRemainingUsd").unwrap_or_default();
    let has_limit: bool = value(properties, "HasLimit").unwrap_or(false);
    let limit_used_pct = (has_limit && limit_usd > 0.0).then(|| {
        ((limit_usd - remaining).max(0.0) / limit_usd * 100.0)
            .round()
            .clamp(0.0, 100.0) as u8
    });
    Some(ProviderUsage {
        id,
        path: path.to_owned(),
        name: value(properties, "Name").unwrap_or_default(),
        kind: value(properties, "Kind").unwrap_or_default(),
        enabled: value(properties, "Enabled").unwrap_or_default(),
        state: value(properties, "State").unwrap_or_default(),
        label: value(properties, "Label").unwrap_or_default(),
        has_limit,
        limit_used_pct,
        limit_usd,
        limit_remaining_usd: remaining,
        limit_reset: value(properties, "LimitReset").unwrap_or_default(),
        include_byok_in_limit: value(properties, "IncludeByokInLimit").unwrap_or_default(),
        usage_usd: value(properties, "UsageUsd").unwrap_or_default(),
        usage_daily_usd: value(properties, "UsageDailyUsd").unwrap_or_default(),
        usage_weekly_usd: value(properties, "UsageWeeklyUsd").unwrap_or_default(),
        usage_monthly_usd: value(properties, "UsageMonthlyUsd").unwrap_or_default(),
        byok_usage_usd: value(properties, "ByokUsageUsd").unwrap_or_default(),
        byok_usage_daily_usd: value(properties, "ByokUsageDailyUsd").unwrap_or_default(),
        byok_usage_weekly_usd: value(properties, "ByokUsageWeeklyUsd").unwrap_or_default(),
        byok_usage_monthly_usd: value(properties, "ByokUsageMonthlyUsd").unwrap_or_default(),
        is_free_tier: value(properties, "IsFreeTier").unwrap_or_default(),
        free_model_daily_used: value(properties, "FreeModelDailyUsed").unwrap_or_default(),
        free_model_daily_limit: value(properties, "FreeModelDailyLimit").unwrap_or_default(),
        free_model_daily_remaining: value(properties, "FreeModelDailyRemaining")
            .unwrap_or_default(),
        last_attempt_unix: value(properties, "LastAttemptUnix").unwrap_or_default(),
        last_success_unix: value(properties, "LastSuccessUnix").unwrap_or_default(),
        last_http_status: value(properties, "LastHttpStatus").unwrap_or_default(),
        refresh_error: value(properties, "RefreshError").unwrap_or_default(),
        stale: value(properties, "Stale").unwrap_or_default(),
        revision: value(properties, "Revision").unwrap_or_default(),
    })
}

pub fn provider_icon_override(id: String) -> Observable<Option<String>> {
    let subject = provider_subject(id);
    let filter = subject.clone();
    watch_relation(vec![subject], ICON_OVERRIDE_RELATION)
        .map(move |records| {
            records
                .iter()
                .find(|record| record.subject == filter)
                .and_then(glyph_from_record)
        })
        .distinct_until_changed()
        .box_it()
}

pub fn set_provider_icon(id: String, glyph: String, picker_input: String) {
    crate::write::spawn("provider icon override", async move {
        set_provider_icon_async(id, glyph, picker_input).await
    });
}

pub fn clear_provider_icon(id: String) {
    crate::write::spawn("provider icon clear", async move {
        let connection = zbus::Connection::session()
            .await
            .map_err(|error| format!("connect session bus: {error}"))?;
        let proxy = locus::RelationsProxy::new(&connection)
            .await
            .map_err(|error| format!("connect locus proxy: {error}"))?;
        proxy
            .clear(provider_subject(id), ICON_OVERRIDE_RELATION)
            .await
            .map(|_| ())
            .map_err(|error| format!("clear locus provider icon relation: {error}"))
    });
}

pub fn refresh(id: String) {
    crate::write::spawn(
        "provider usage refresh",
        async move { refresh_async(id).await },
    );
}

async fn refresh_async(id: String) -> Result<(), String> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    let proxy = zbus::Proxy::new(&connection, BUS_NAME, PROVIDERS_PATH, PROVIDERS_INTERFACE)
        .await
        .map_err(|error| format!("connect provider manager: {error}"))?;
    proxy
        .call::<_, _, ()>("RefreshProvider", &(id,))
        .await
        .map_err(|error| format!("refresh provider usage: {error}"))
}

async fn set_provider_icon_async(
    id: String,
    glyph: String,
    picker_input: String,
) -> Result<(), String> {
    let glyph = crate::text::trimmed(glyph).ok_or_else(|| "empty provider icon".to_owned())?;
    let connection = zbus::Connection::session()
        .await
        .map_err(|error| format!("connect session bus: {error}"))?;
    let proxy = locus::RelationsProxy::new(&connection)
        .await
        .map_err(|error| format!("connect locus proxy: {error}"))?;
    let mut metadata = HashMap::new();
    if let Some(input) = crate::text::trimmed(picker_input) {
        metadata.insert(PICKER_INPUT_METADATA.to_owned(), input);
    }
    proxy
        .set_one_with_persistence(
            provider_subject(id),
            ICON_OVERRIDE_RELATION,
            RelationEndpoint::stable_key(ICON_GLYPH_KIND, glyph),
            metadata,
            true,
        )
        .await
        .map(|_| ())
        .map_err(|error| format!("set locus provider icon relation: {error}"))
}

fn glyph_from_record(record: &RelationRecord) -> Option<String> {
    let RelationEndpoint::StableKey { kind, id } = &record.target else {
        return None;
    };
    (kind == ICON_GLYPH_KIND)
        .then(|| crate::text::non_empty(id))
        .flatten()
        .map(str::to_owned)
}

fn provider_subject(id: String) -> RelationEndpoint {
    RelationEndpoint::stable_key(PROVIDER_ID_KIND, id)
}

fn money(value: f64) -> String {
    format!("${value:.2}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_percent_only_exists_for_limited_keys() {
        let mut usage = ProviderUsage::default();
        assert_eq!(usage.bar_percent(), None);
        usage.has_limit = true;
        usage.limit_used_pct = Some(42);
        assert_eq!(usage.bar_percent(), Some(42));
    }
}
