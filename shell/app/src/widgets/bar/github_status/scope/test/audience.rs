use super::*;
use futures_util::{Stream, StreamExt};
use shell_core::source::StateSignal;
use std::time::Duration;

async fn next<T>(stream: &mut (impl Stream<Item = Result<T, String>> + Unpin)) -> T {
    tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("inventory did not update")
        .unwrap()
        .unwrap()
}

#[test]
fn reviews_never_construct_the_project_graph_and_authored_never_uses_review_graph() {
    for category in Category::ALL {
        let inventory = if category.is_activity() {
            select_inventory(
                category,
                || panic!("review category attempted to observe project tags"),
                || source::once(Some(vec![])),
            )
        } else {
            select_inventory(
                category,
                || source::once(None),
                || panic!("authored category attempted to use tagless review scope"),
            )
        };
        drop(inventory);
    }
}

#[test]
fn mismatched_tags_project_loss_and_tag_updates_do_not_hide_review_work() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let own_a = pr(1, PrStatus::Approved, &["a"]);
        let own_b = pr(2, PrStatus::Approved, &["b"]);
        let mut initial = pr(3, PrStatus::CiFailed, &["unrelated"]);
        initial.pr.review_requested = true;
        initial.pr.review_activity = Some(github::ReviewActivity::InitialReview);
        let mut rereview = pr(4, PrStatus::Conflicts, &[]);
        rereview.pr.review_requested = true;
        rereview.pr.review_activity = Some(github::ReviewActivity::ReReview);
        let mut waiting = pr(5, PrStatus::CiPending, &["other-project"]);
        waiting.pr.review_activity = Some(github::ReviewActivity::ChangesRequested);
        let rows = vec![
            own_a.clone(),
            own_b.clone(),
            initial.clone(),
            rereview.clone(),
            waiting.clone(),
        ];
        let activity = StateSignal::new(Some(rows.clone()));
        let tags = StateSignal::new(None); // Project/labels unavailable initially.
        let tagless = StateSignal::new(urls(&rows));
        let bucket = |category| {
            select_inventory(
                category,
                || scoped_activity(activity.observable(), tags.observable()),
                || scoped_activity(activity.observable(), tagless.observable()),
            )
            .map(move |rows| view(category, rows))
            .box_it()
            .into_stream()
        };
        let mut own = bucket(Category::Approved);
        let mut first = bucket(Category::InitialReview);
        let mut again = bucket(Category::ReReview);
        let mut changes = bucket(Category::MyChangesRequested);
        assert!(!next(&mut own).await.available);
        assert_eq!(next(&mut first).await.rows[0].url, initial.pr.url);
        assert_eq!(next(&mut again).await.rows[0].url, rereview.pr.url);
        assert_eq!(next(&mut changes).await.rows[0].url, waiting.pr.url);

        tags.set(urls(std::slice::from_ref(&own_a)));
        assert_eq!(next(&mut own).await.rows[0].url, own_a.pr.url);
        tags.set(Some(vec![])); // Tags with no matches do not make reviews empty.
        assert_eq!(next(&mut own).await.count(), "0");
        tags.set(None); // Switching projects/losing projd still cannot hide reviews.
        assert!(!next(&mut own).await.available);
        tags.set(urls(std::slice::from_ref(&own_b)));
        assert_eq!(next(&mut own).await.rows[0].url, own_b.pr.url);

        // An independent activity update proves all three reviews remain global
        // across those tag changes; acknowledgment changes badges, not membership.
        let mut updated = rows.clone();
        for row in &mut updated[2..] {
            row.unseen = false;
        }
        activity.set(Some(updated.clone()));
        for (stream, original) in [
            (&mut first, &initial),
            (&mut again, &rereview),
            (&mut changes, &waiting),
        ] {
            let shown = next(stream).await;
            assert_eq!(shown.count(), "1");
            assert_eq!(shown.unseen_count(), 0);
            assert_eq!(shown.rows[0].url, original.pr.url);
            let live = updated
                .iter()
                .find(|item| item.pr.url == original.pr.url)
                .cloned();
            assert_eq!(
                row_view(Category::for_pr(&original.pr), live).revision,
                original.revision
            );
        }

        // Only the tagless Listener membership governs review removal.
        tagless.set(urls(&[own_a, own_b, rereview.clone(), waiting.clone()]));
        assert_eq!(next(&mut first).await.count(), "0");
        assert_eq!(next(&mut again).await.count(), "1");
        assert_eq!(next(&mut changes).await.count(), "1");
        tagless.set(None);
        assert!(!next(&mut first).await.available);
        assert!(!next(&mut again).await.available);
        assert!(!next(&mut changes).await.available);
    });
}
