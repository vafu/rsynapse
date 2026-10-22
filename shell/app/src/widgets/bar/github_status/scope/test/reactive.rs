use super::*;
use futures_util::{Stream, StreamExt};
use shell_core::source::StateSignal;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

async fn next<T>(stream: &mut (impl Stream<Item = Result<T, String>> + Unpin)) -> T {
    tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("source did not update")
        .unwrap()
        .unwrap()
}

fn project(id: &str) -> ProjectScope {
    ProjectScope::Project(id.into())
}

#[test]
fn singleton_listeners_union_membership_and_switch_projects_without_stale_rows() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let first = pr(1, PrStatus::Approved, &[]);
        let second = pr(2, PrStatus::Draft, &[]);
        let activity = StateSignal::new(Some(vec![first.clone(), second.clone()]));
        let selected = StateSignal::new(project("first"));
        let a_labels = StateSignal::new(Some(vec!["a".into(), "b".into(), "a".into()]));
        let b_labels = StateSignal::new(None);
        let a_members = StateSignal::new(urls(std::slice::from_ref(&first)));
        let b_members = StateSignal::new(urls(&[first.clone(), second.clone()]));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let filter = project_filter(selected.observable(), {
            let (a, b) = (a_labels.clone(), b_labels.clone());
            move |id| {
                if id == "first" {
                    a.observable()
                } else {
                    b.observable()
                }
            }
        });
        let membership = listener_membership(filter, {
            let (a, b, all, requests) = (
                a_members.clone(),
                b_members.clone(),
                urls(&[first.clone(), second.clone()]),
                requests.clone(),
            );
            move |labels| {
                requests.lock().unwrap().push(labels.clone());
                match labels.first().map(String::as_str) {
                    Some("a") => a.observable(),
                    Some("b") => b.observable(),
                    None => source::once(all.clone()),
                    _ => source::once(Some(vec![])),
                }
            }
        });
        let mut stream = scoped_activity(activity.observable(), membership).into_stream();
        assert_eq!(next(&mut stream).await, None);
        assert_eq!(next(&mut stream).await, activity.get());
        assert_eq!(*requests.lock().unwrap(), vec![vec!["a"], vec!["b"]]);

        // Membership alone moves a PR, without any change to its label metadata.
        a_members.set(Some(vec![]));
        b_members.set(urls(std::slice::from_ref(&second)));
        assert_eq!(next(&mut stream).await, Some(vec![second.clone()]));
        b_members.set(None);
        assert_eq!(next(&mut stream).await, None);
        b_members.set(Some(vec![]));
        assert_eq!(next(&mut stream).await, Some(vec![]));

        selected.set(project("second"));
        assert_eq!(next(&mut stream).await, None);
        a_labels.set(Some(vec![])); // Old project cannot enable global fallback.
        b_labels.set(Some(vec!["absent".into()]));
        assert_eq!(next(&mut stream).await, Some(vec![]));
        b_labels.set(Some(vec![]));
        assert_eq!(next(&mut stream).await, None);
        assert_eq!(next(&mut stream).await, activity.get());
        assert_eq!(
            requests.lock().unwrap().last().unwrap(),
            &Vec::<String>::new()
        );
        selected.set(ProjectScope::Unavailable);
        assert_eq!(next(&mut stream).await, None);
        selected.set(ProjectScope::Global);
        assert_eq!(next(&mut stream).await, activity.get());
    });
}

#[test]
fn independent_rails_follow_membership_and_activity_revisions() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let first = pr(1, PrStatus::Merged, &[]);
        let second = pr(2, PrStatus::CiFailed, &[]);
        let activity = StateSignal::new(Some(vec![first.clone(), second.clone()]));
        let left_members = StateSignal::new(urls(std::slice::from_ref(&first)));
        let right_members = StateSignal::new(urls(std::slice::from_ref(&second)));
        let mut left =
            scoped_activity(activity.observable(), left_members.observable()).into_stream();
        let mut right =
            scoped_activity(activity.observable(), right_members.observable()).into_stream();
        assert_eq!(next(&mut left).await, Some(vec![first.clone()]));
        assert_eq!(next(&mut right).await, Some(vec![second.clone()]));
        let mut seen = second.clone();
        seen.unseen = false;
        activity.set(Some(vec![first.clone(), seen.clone()]));
        assert_eq!(next(&mut right).await, Some(vec![seen.clone()]));
        assert_eq!(seen.revision, second.revision);
        assert_eq!(
            view(Category::Failed, Some(vec![seen.clone()])).unseen_count(),
            0
        );
        right_members.set(urls(std::slice::from_ref(&first)));
        assert_eq!(next(&mut right).await, Some(vec![first]));
        activity.set(Some(vec![seen])); // Authoritative merged dismissal.
        assert_eq!(next(&mut left).await, Some(vec![]));
        assert_eq!(next(&mut right).await, Some(vec![]));
        activity.set(None);
        assert_eq!(next(&mut left).await, None);
        assert_eq!(next(&mut right).await, None);
    });
}

#[test]
fn switching_empty_workspaces_changes_project_without_window_state() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let workspace = |id: &str, project_id: &str| {
            Some(SelectedWorkspaceView {
                workspace_id: Some(id.into()),
                visible: true,
                project: Some(ProjectView {
                    project_id: Some(project_id.into()),
                    visible: true,
                    ..Default::default()
                }),
                ..Default::default()
            })
        };
        let first = pr(1, PrStatus::Approved, &[]);
        let second = pr(2, PrStatus::Approved, &[]);
        let selected = StateSignal::new(workspace("empty-1", "a"));
        let filter = project_filter(selected.observable().map(project_scope).box_it(), |id| {
            source::once(Some(vec![id]))
        });
        let membership = listener_membership(filter, {
            let (a, b) = (
                urls(std::slice::from_ref(&first)),
                urls(std::slice::from_ref(&second)),
            );
            move |labels| {
                source::once(if labels == ["a"] {
                    a.clone()
                } else {
                    b.clone()
                })
            }
        });
        let mut stream = scoped_activity(
            source::once(Some(vec![first.clone(), second.clone()])),
            membership,
        )
        .into_stream();
        assert_eq!(next(&mut stream).await, None);
        assert_eq!(next(&mut stream).await, Some(vec![first.clone()]));
        selected.set(workspace("empty-2", "b"));
        assert_eq!(next(&mut stream).await, None);
        assert_eq!(next(&mut stream).await, Some(vec![second]));
        selected.set(workspace("empty-3", "a"));
        assert_eq!(next(&mut stream).await, None);
        assert_eq!(next(&mut stream).await, Some(vec![first]));
    });
}
