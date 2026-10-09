use super::{Bindings, PROJECT_AGENT, WINDOW_PROJECT};
use crate::relations::{self, LocusClient};
use locus::{RelationEndpoint, RelationRecord};
use std::collections::HashSet;

async fn clear(locus: &LocusClient, record: RelationRecord) -> anyhow::Result<()> {
    if record.relation == relations::WORKSPACE_PROJECT {
        if let RelationEndpoint::StableKey { kind, id } = &record.subject {
            if kind == locus::keys::NIRI_WORKSPACE_ID {
                Bindings {
                    locus: locus.clone(),
                }
                .unassign_workspace_project(id.parse()?)
                .await?;
                return Ok(());
            }
        }
    }
    locus
        .unset(record.subject, &record.relation, record.target)
        .await
}
pub(super) async fn removed(
    locus: &LocusClient,
    project: goal_model::RemovedProjectInfo,
) -> anyhow::Result<()> {
    for relation in [relations::WORKSPACE_PROJECT, WINDOW_PROJECT] {
        for record in locus.list(relation).await? {
            if record.metadata.get("project-id") == Some(&project.project.id)
                || matches!(&record.target,RelationEndpoint::StableKey{kind,id} if kind==locus::keys::PROJECT_PATH && (id==&project.project.cwd||project.checkouts.iter().any(|c|&c.root_path==id)))
            {
                clear(locus, record).await?;
            }
        }
    }
    reconcile_agents(
        locus,
        &HashSet::from([goal_model::object_path("Projects", &project.project.id)]),
        true,
    )
    .await
}
pub(super) async fn reconcile(
    locus: &LocusClient,
    projects: Vec<goal_model::ProjectInfo>,
) -> anyhow::Result<()> {
    let ids: HashSet<_> = projects.iter().map(|p| p.id.clone()).collect();
    for relation in [relations::WORKSPACE_PROJECT, WINDOW_PROJECT] {
        for state in locus.list_with_persistence(relation).await? {
            let mut record = state.record;
            let linked = record
                .metadata
                .get("checkout-id")
                .filter(|id| !id.is_empty())
                .and_then(|id| projects.iter().find(|p| &p.checkout_id == id))
                .or_else(|| match &record.target {
                    RelationEndpoint::StableKey { kind, id }
                        if kind == locus::keys::PROJECT_PATH =>
                    {
                        projects.iter().find(|p| &p.cwd == id)
                    }
                    _ => None,
                });
            if let Some(project) = linked {
                let changed = record.metadata.get("project-id") != Some(&project.id)
                    || record.metadata.contains_key("context-id");
                if changed {
                    record
                        .metadata
                        .insert("project-id".into(), project.id.clone());
                    record
                        .metadata
                        .insert("checkout-id".into(), project.checkout_id.clone());
                    record.metadata.remove("context-id");
                    locus
                        .set_one_with_persistence(
                            record.subject.clone(),
                            relation,
                            record.target.clone(),
                            record.metadata.clone(),
                            state.persist,
                        )
                        .await?;
                }
            }
            if record
                .metadata
                .get("project-id")
                .is_some_and(|id| !ids.contains(id))
            {
                clear(locus, record).await?;
            }
        }
    }
    super::icons::migrate(locus).await?;
    for state in locus.list_with_persistence(PROJECT_AGENT).await? {
        let mut record = state.record.clone();
        if let RelationEndpoint::DBusObject {
            service,
            path,
            interface,
            ..
        } = &mut record.subject
        {
            if service == goal_model::BUS_NAME && interface == goal_model::PROJECT_INTERFACE {
                if let Some(project) = projects.iter().find(|p| {
                    let encoded: String = p.id.bytes().map(|b| format!("{b:02x}")).collect();
                    *path == format!("{}/Projects/n{encoded}", goal_model::ROOT_PATH)
                }) {
                    *path = goal_model::object_path("Projects", &project.id);
                    locus
                        .set_with_persistence(
                            record.subject.clone(),
                            PROJECT_AGENT,
                            record.target.clone(),
                            record.metadata.clone(),
                            state.persist,
                        )
                        .await?;
                    locus
                        .unset(state.record.subject, PROJECT_AGENT, state.record.target)
                        .await?;
                }
            }
        }
    }
    let paths = projects
        .iter()
        .map(|p| goal_model::object_path("Projects", &p.id))
        .collect();
    reconcile_agents(locus, &paths, false).await
}
async fn reconcile_agents(
    locus: &LocusClient,
    paths: &HashSet<String>,
    matching: bool,
) -> anyhow::Result<()> {
    for record in locus.list(PROJECT_AGENT).await? {
        if let RelationEndpoint::DBusObject {
            service,
            path,
            interface,
            ..
        } = &record.subject
        {
            if service == goal_model::BUS_NAME
                && interface == goal_model::PROJECT_INTERFACE
                && paths.contains(path) == matching
            {
                clear(locus, record).await?;
            }
        }
    }
    Ok(())
}
