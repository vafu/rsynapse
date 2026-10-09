//! One-time adoption of workspace overrides by associated projects.
use crate::relations::{self, LocusClient};
use locus::RelationEndpoint;

const OVERRIDE: &str = "org.rsynapse.workspace.icon-override";

pub(super) async fn migrate(locus: &LocusClient) -> anyhow::Result<()> {
    let icons = locus.list(OVERRIDE).await?;
    if icons.is_empty() {
        return Ok(());
    }
    let names = locus.list(relations::WORKSPACE_NAME).await?;
    let mut bindings = locus.list(relations::WORKSPACE_PROJECT).await?;
    bindings.sort_by_key(|r| format!("{:?}", r.subject));
    let proj = super::proj().await?;
    let connection = zbus::Connection::session().await?;
    let mut adopted = Vec::new();
    for binding in bindings {
        let Some(project) = binding.metadata.get("project-id") else {
            continue;
        };
        let mut subjects = Vec::new();
        // Old pickers keyed named workspaces by their compositor name.
        if let RelationEndpoint::StableKey { kind, id } = &binding.subject {
            if kind == locus::keys::NIRI_WORKSPACE_ID {
                let path = format!("/org/rsynapse/Niri/Workspaces/workspace_{id}");
                if let Ok(proxy) = zbus::Proxy::new(
                    &connection,
                    niri_dbus::BUS_NAME,
                    path.as_str(),
                    "org.rsynapse.Niri1.Workspace",
                )
                .await
                {
                    if let Ok(names) = proxy.get_property::<Vec<String>>("Name").await {
                        subjects.extend(names.into_iter().map(|name| {
                            RelationEndpoint::stable_key(locus::keys::NIRI_WORKSPACE_NAME, name)
                        }));
                    }
                }
            }
        }
        if let Some(name) = names.iter().find(|r| r.subject == binding.subject) {
            if let RelationEndpoint::StableKey { id, .. } = &name.target {
                subjects.push(RelationEndpoint::stable_key(
                    locus::keys::NIRI_WORKSPACE_NAME,
                    id.clone(),
                ));
            }
        }
        subjects.push(binding.subject);
        for subject in subjects {
            if let Some(icon) = icons.iter().find(|r| r.subject == subject) {
                if let RelationEndpoint::StableKey { kind, id } = &icon.target {
                    if kind == "org.rsynapse.icon.glyph" {
                        let result: goal_model::ProjectInfo =
                            proj.call("AdoptProjectIcon", &(project, id)).await?;
                        // Preserve conflicting legacy choices rather than silently discarding them.
                        if result.icon == *id {
                            adopted.push(icon.clone());
                        }
                        break;
                    }
                }
            }
        }
    }
    for icon in adopted {
        locus.unset(icon.subject, OVERRIDE, icon.target).await?;
    }
    Ok(())
}
