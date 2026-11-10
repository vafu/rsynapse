use std::collections::HashMap;

use data_sources::{self, Observable, rx::Observable as _, text::trimmed};
use locus::{RelationEndpoint, RelationRecord, keys};

const WINDOW_APP_INSTANCE_RELATION: &str = "org.rsynapse.window.app-instance";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct AppInstance {
    pub(super) name: Option<String>,
    pub(super) icon: Option<String>,
}

pub(super) fn app_instance_for_window(window_id: String) -> Observable<AppInstance> {
    let subject = RelationEndpoint::stable_key(keys::WINDOW_ID, window_id);
    let filter_subject = subject.clone();
    data_sources::watch_relation_subject(subject, WINDOW_APP_INSTANCE_RELATION)
        .map(move |records| {
            records
                .into_iter()
                .find(|record| record.subject == filter_subject)
                .map(AppInstance::from)
                .unwrap_or_default()
        })
        .distinct_until_changed()
        .box_it()
}

impl From<RelationRecord> for AppInstance {
    fn from(record: RelationRecord) -> Self {
        Self {
            name: metadata_value(&record.metadata, &["app-name"]),
            icon: metadata_value(&record.metadata, &["app-icon"]),
        }
    }
}

fn metadata_value(metadata: &HashMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| metadata.get(*key).cloned().and_then(trimmed))
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}
