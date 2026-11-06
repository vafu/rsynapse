//! Reactive preferred workspace names from locus.
use locus::RelationEndpoint;

use crate::{Observable, rx::Observable as _};

const WORKSPACE_NAME_RELATION: &str = workspace_policy::NAME_RELATION;
const WORKSPACE_NAME_KIND: &str = workspace_policy::NAME_KIND;

/// Preferred manual name for one workspace id from locus. Automatic names
/// fall back to the stored project name or `empty` in the view.
pub fn workspace_display_name(workspace_id: Option<String>) -> Observable<Option<String>> {
    let Some(id) = workspace_id else {
        return crate::once(None);
    };
    let subject = RelationEndpoint::stable_key(locus::keys::WORKSPACE_ID, id.clone());
    crate::shared_by_key("rsynapse.workspace-name", id.to_string(), move || {
        watch_workspace_name(subject.clone())
            .distinct_until_changed()
            .box_it()
    })
}

fn watch_workspace_name(subject: RelationEndpoint) -> Observable<Option<String>> {
    let filter_subject = subject.clone();
    crate::watch_relation_subject(subject, WORKSPACE_NAME_RELATION)
        .map(move |records| {
            records
                .into_iter()
                .find(|record| record.subject == filter_subject)
                .and_then(|record| name_of(&record))
        })
        .distinct_until_changed()
        .box_it()
}

use workspace_policy::name_of;

#[cfg(test)]
mod tests {
    use super::*;
    use locus::RelationRecord;
    use std::collections::HashMap;

    #[test]
    fn only_manual_names_override_project_cwd_or_empty() {
        let mut record = RelationRecord {
            subject: RelationEndpoint::stable_key(locus::keys::WORKSPACE_ID, "7"),
            relation: WORKSPACE_NAME_RELATION.to_owned(),
            target: RelationEndpoint::stable_key(WORKSPACE_NAME_KIND, "noble-owl"),
            metadata: HashMap::from([("source".to_owned(), "random".to_owned())]),
            created_at_unix_ms: 0,
            updated_at_unix_ms: 0,
        };
        assert_eq!(name_of(&record), None);
        record
            .metadata
            .insert("source".to_owned(), "project".to_owned());
        assert_eq!(name_of(&record), None);
        record
            .metadata
            .insert("source".to_owned(), "manual".to_owned());
        assert_eq!(name_of(&record), Some("noble-owl".to_owned()));
    }
}
