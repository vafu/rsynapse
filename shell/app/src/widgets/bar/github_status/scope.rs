use shell_core::source::{self, Observable, github, rx::Observable as _};
use std::collections::BTreeSet;

use super::Scope;
use crate::widgets::bar::selected_project::{SelectedWorkspaceView, selected_project_context};

#[derive(Clone, Debug, Eq, PartialEq)]
enum ProjectScope {
    Unavailable,
    Global,
    Project(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum LabelFilter {
    Unavailable,
    Global,
    Labels(Vec<String>),
}

type Inventory = Option<Vec<github::PullRequestActivity>>;

pub(super) fn inventory(scope: Scope) -> Observable<Inventory> {
    source::shared_by_key(
        "rsynapse.github-project-activity",
        format!("{scope:?}"),
        move || {
            let project = selected_project_context(scope.output_name.clone())
                .map(project_scope)
                .distinct_until_changed()
                .box_it();
            scoped_activity(
                github::activity(),
                listener_membership(
                    project_filter(project, |id| source::proj::project_labels(id)),
                    github::listener,
                ),
            )
        },
    )
}

fn project_scope(workspace: Option<SelectedWorkspaceView>) -> ProjectScope {
    let Some(workspace) = workspace else {
        return ProjectScope::Unavailable;
    };
    match workspace.project {
        None => ProjectScope::Global,
        Some(project) => project
            .project_id
            .filter(|id| !id.is_empty())
            .map(ProjectScope::Project)
            .unwrap_or(ProjectScope::Unavailable),
    }
}

fn project_filter(
    project: Observable<ProjectScope>,
    labels: impl Fn(String) -> Observable<Option<Vec<String>>> + Send + Sync + 'static,
) -> Observable<LabelFilter> {
    source::switch_map(
        project.distinct_until_changed().box_it(),
        move |project| match project {
            ProjectScope::Unavailable => source::once(LabelFilter::Unavailable),
            ProjectScope::Global => source::once(LabelFilter::Global),
            ProjectScope::Project(id) => labels(id)
                .map(|labels| match labels {
                    None => LabelFilter::Unavailable,
                    Some(labels) if labels.is_empty() => LabelFilter::Global,
                    Some(labels) => LabelFilter::Labels(labels),
                })
                // Never replay the previous project's matches while Labels loads.
                .start_with(vec![LabelFilter::Unavailable])
                .box_it(),
        },
    )
    .distinct_until_changed()
    .box_it()
}

fn scoped_activity(
    activity: Observable<Inventory>,
    membership: Observable<Option<Vec<String>>>,
) -> Observable<Inventory> {
    activity
        .combine_latest(membership, filter_inventory)
        .distinct_until_changed()
        .box_it()
}

fn listener_membership(
    filter: Observable<LabelFilter>,
    listen: impl Fn(Vec<String>) -> Observable<Option<Vec<String>>> + Send + Sync + 'static,
) -> Observable<Option<Vec<String>>> {
    source::switch_map(filter, move |filter| {
        let filters: Vec<Vec<String>> = match filter {
            LabelFilter::Unavailable => return source::once(None),
            LabelFilter::Global => vec![vec![]],
            LabelFilter::Labels(labels) => {
                let labels: BTreeSet<_> = labels.into_iter().collect();
                if labels.is_empty() {
                    vec![vec![]]
                } else {
                    // The factory matches ALL labels. Singleton listeners plus
                    // a union preserve the project's ANY-label policy.
                    labels.into_iter().map(|label| vec![label]).collect()
                }
            }
        };
        source::combine_latest(filters.into_iter().map(&listen).collect())
            .map(|memberships| {
                memberships
                    .into_iter()
                    .collect::<Option<Vec<_>>>()
                    .map(|sets| {
                        sets.into_iter()
                            .flatten()
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .collect()
                    })
            })
            .start_with(vec![None])
            .box_it()
    })
    .distinct_until_changed()
    .box_it()
}

fn filter_inventory(activity: Inventory, membership: Option<Vec<String>>) -> Inventory {
    let (prs, urls) = activity.zip(membership)?;
    let urls: BTreeSet<_> = urls.into_iter().collect();
    Some(
        prs.into_iter()
            .filter(|item| urls.contains(&item.pr.url))
            .collect(),
    )
}

#[cfg(test)]
mod test;
