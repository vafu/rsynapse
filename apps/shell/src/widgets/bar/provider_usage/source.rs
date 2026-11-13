use data_sources::{self, Observable, provider_usage::ProviderUsage, rx::Observable as _};
use shell_rx_macros::combine_latest;

use super::super::icon_resolver::{
    IconChoice, IconEvidence, IconEvidenceKind, IconPolicy, IconRequest, IconResolution,
    resolve_icon,
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProviderUsageVm {
    pub usage: ProviderUsage,
    pub icon: IconResolution,
    pub percent: Option<u8>,
}

#[derive(Clone, Debug, PartialEq)]
struct Base {
    usage: ProviderUsage,
    override_icon: Option<String>,
}

pub(crate) fn provider_usage_keys() -> Observable<Vec<String>> {
    data_sources::provider_usage::providers()
        .map(|providers| providers.into_iter().map(|provider| provider.id).collect())
        .distinct_until_changed()
        .box_it()
}

pub(crate) fn provider_usage(id: String) -> Observable<Option<ProviderUsageVm>> {
    let key = id.clone();
    data_sources::shared_by_key("rsynapse.provider-usage-item", key, move || {
        let provider_id = id.clone();
        let icon_id = id.clone();
        let provider: Observable<Option<ProviderUsage>> = data_sources::provider_usage::providers()
            .map(move |providers| {
                providers
                    .into_iter()
                    .find(|provider| provider.id == provider_id)
            })
            .distinct_until_changed()
            .box_it();
        let override_icon: Observable<Option<String>> = data_sources::switch_map(
            data_sources::provider_usage::providers()
                .map(move |providers| {
                    providers
                        .into_iter()
                        .find(|provider| provider.id == icon_id)
                        .map(|provider| provider.id)
                        .unwrap_or_default()
                })
                .box_it(),
            data_sources::provider_usage::provider_icon_override,
        )
        .box_it();
        let base = combine_latest!(provider, override_icon => |(provider, override_icon)| {
            provider.map(move |usage| Base {
                usage,
                override_icon: override_icon.clone(),
            })
        })
        .box_it();
        data_sources::switch_map(base, |base| match base {
            Some(base) => {
                let request = icon_request(&base.usage, base.override_icon);
                let usage = base.usage;
                resolve_icon(request)
                    .map(move |icon| {
                        Some(ProviderUsageVm {
                            percent: usage.bar_percent(),
                            usage: usage.clone(),
                            icon,
                        })
                    })
                    .distinct_until_changed()
                    .box_it()
            }
            None => data_sources::once(None::<ProviderUsageVm>),
        })
        .distinct_until_changed()
        .box_it()
    })
}

fn icon_request(provider: &ProviderUsage, override_icon: Option<String>) -> IconRequest {
    let mut request = IconRequest::new(
        format!("provider-icon:{}", provider.id),
        IconChoice::new(crate::widgets::nerd_icon::md::MD_SERVER.to_owned())
            .expect("static icon is non-empty"),
        IconPolicy::window_app(),
        vec![
            IconEvidence::new(IconEvidenceKind::AppId, provider.id.clone()),
            IconEvidence::new(IconEvidenceKind::DesktopName, provider.name.clone()),
        ]
        .into_iter()
        .flatten()
        .collect(),
    );
    if let Some(icon) = override_icon.and_then(IconChoice::new) {
        request = request.with_override(Some(icon));
    }
    request
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_list_is_stable() {
        let providers = vec![ProviderUsage {
            id: "openrouter-snap".to_owned(),
            ..ProviderUsage::default()
        }];
        assert_eq!(
            providers
                .into_iter()
                .map(|provider| provider.id)
                .collect::<Vec<_>>(),
            vec!["openrouter-snap".to_owned()]
        );
    }
}
