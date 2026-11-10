use crate::{Manager, ProjectObject, error, path};
use std::collections::BTreeSet;
use zbus::{Connection, object_server::SignalContext};

fn normalize(labels: Vec<String>) -> zbus::fdo::Result<Vec<String>> {
    labels
        .into_iter()
        .map(|label| {
            let trimmed = label.trim();
            if trimmed.is_empty() || label.chars().any(char::is_control) {
                return Err(zbus::fdo::Error::InvalidArgs(
                    "Labels must be nonempty and contain no control characters".into(),
                ));
            }
            Ok(trimmed.to_owned())
        })
        .collect::<zbus::fdo::Result<BTreeSet<_>>>()
        .map(|labels| labels.into_iter().collect())
}

impl Manager {
    pub(super) async fn edit_labels(
        &self,
        id: &str,
        labels: Vec<String>,
        add: bool,
        conn: &Connection,
        e: &SignalContext<'_>,
    ) -> zbus::fdo::Result<Vec<String>> {
        let labels = normalize(labels)?;
        let _lock = self.mutation.lock().await;
        let (labels, changed) = self
            .store
            .edit_project_labels(id, labels, add)
            .map_err(error)?
            .ok_or_else(|| zbus::fdo::Error::FileNotFound("Project not found".into()))?;
        if changed {
            let handle = conn
                .object_server()
                .interface::<_, ProjectObject>(path("Projects", id))
                .await
                .map_err(error)?;
            handle
                .get()
                .await
                .labels_changed(handle.signal_context())
                .await
                .map_err(error)?;
            Self::changed(e, "project", id).await.map_err(error)?;
        }
        Ok(labels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_preserve_exact_spelling_and_internal_whitespace() {
        assert_eq!(
            normalize(vec![
                " bug ".into(),
                "Bug".into(),
                "bug".into(),
                "team  name".into(),
                "\u{2003}équipe\u{2003}".into()
            ])
            .unwrap(),
            vec!["Bug", "bug", "team  name", "équipe"]
        );
        for bad in ["", "  ", "\tbug", "bug\n", "a\u{7f}b", "a\u{85}b"] {
            assert!(matches!(
                normalize(vec!["valid".into(), bad.into()]),
                Err(zbus::fdo::Error::InvalidArgs(_))
            ));
        }
    }
}
