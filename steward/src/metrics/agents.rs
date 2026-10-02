use locus::{RelationEndpoint, RelationRecord};
use shell_rx_macros::combine_latest;
use shell_source::{
    Observable,
    dbus::{self, Bus, ObjectDescriptor, PropertyDescriptor},
    rx::Observable as _,
};

const BUS: &str = "io.github.AgentDBus";
const INTERFACE: &str = "io.github.AgentDBus1.Session";

/// Locus supplies the link; AgentDBus remains authoritative for live ownership
/// and agent identity. Watch only the selected session's two properties.
pub(super) fn agent_app(
    records: Observable<Vec<RelationRecord>>,
    window: Option<u64>,
) -> Observable<Option<String>> {
    let Some(window) = window else {
        return shell_source::once(None);
    };
    let paths = records
        .map(move |records| session_path(&records, window))
        .distinct_until_changed()
        .box_it();
    shell_source::switch_map(paths, move |path| {
        let Some(path) = path else {
            return shell_source::once(None);
        };
        let Ok(object) = ObjectDescriptor::parse(Bus::Session, BUS, &path, INTERFACE) else {
            return shell_source::once(None);
        };
        combine_latest!(
            dbus::property_or(PropertyDescriptor::new(object.clone(), "WindowId"), String::new()),
            dbus::property_or(PropertyDescriptor::new(object, "AgentName"), String::new())
            => move |(owner, name)| live_name(window, &owner, &name),
        )
        .distinct_until_changed()
        .box_it()
    })
    .start_with(vec![None])
    .distinct_until_changed()
    .box_it()
}

fn session_path(records: &[RelationRecord], window: u64) -> Option<String> {
    let subject = RelationEndpoint::stable_key(locus::keys::NIRI_WINDOW_ID, window.to_string());
    records.iter().find_map(|record| {
        if record.subject != subject {
            return None;
        }
        match &record.target {
            RelationEndpoint::StableKey { kind, .. } if kind == locus::keys::AGENT_SESSION_ID => {
                record.metadata.get("session_path").cloned()
            }
            RelationEndpoint::DBusObject {
                service,
                path,
                interface,
                ..
            } if service == BUS && interface == INTERFACE => Some(path.clone()),
            _ => None,
        }
    })
}

fn live_name(window: u64, owner: &str, name: &str) -> Option<String> {
    if owner.parse::<u64>().ok() != Some(window) {
        return None;
    }
    let name = name.trim();
    (!name.is_empty()).then(|| crate::focus::canonical_app_id(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn opencode_link_resolves_without_app_instance_name() {
        let record = RelationRecord {
            subject: RelationEndpoint::stable_key(locus::keys::NIRI_WINDOW_ID, "87"),
            relation: crate::relations::WINDOW_AGENT_SESSION.to_owned(),
            target: RelationEndpoint::stable_key(locus::keys::AGENT_SESSION_ID, "opencode/ses_123"),
            metadata: HashMap::from([(
                "session_path".to_owned(),
                "/io/github/AgentDBus/sessions/opencode/ses_123".to_owned(),
            )]),
            created_at_unix_ms: 0,
            updated_at_unix_ms: 0,
        };
        assert!(session_path(std::slice::from_ref(&record), 87).is_some());
        assert!(session_path(&[record], 88).is_none());
        assert_eq!(live_name(87, "87", "opencode"), Some("opencode".to_owned()));
    }

    #[test]
    fn stale_or_unowned_sessions_cannot_override_app_identity() {
        assert_eq!(live_name(87, "88", "codex"), None);
        assert_eq!(live_name(87, "", "opencode"), None);
        assert_eq!(live_name(87, "87", " "), None);
    }
}
