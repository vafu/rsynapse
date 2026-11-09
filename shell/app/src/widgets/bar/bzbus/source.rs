use shell_core::source::{self, Observable, bzbus as bzbus_source, rx::Observable as _};
use shell_rx_macros::combine_latest;

use super::view::{self, BzBusView};
use crate::widgets::bar::WindowNode;

pub(in crate::widgets::bar) fn bzbus_for_window(
    window: WindowNode,
) -> Observable<Option<BzBusView>> {
    source::switch_map(window.id().box_it(), |window_id| {
        combine_latest!(
            bzbus_source::snapshots(),
            bzbus_source::invocation_ids_for_window(window_id)
                => |(snapshot, invocation_ids)| {
                    if invocation_ids.is_empty() {
                        return None;
                    }

                    let invocations = snapshot
                        .invocations
                        .into_iter()
                        .filter(|invocation| invocation_ids.iter().any(|id| id == &invocation.id))
                        .collect();
                    Some(view::view(snapshot.active, invocations))
                },
        )
        .distinct_until_changed()
        .box_it()
    })
    .distinct_until_changed()
    .box_it()
}

pub(in crate::widgets::bar) fn ongoing_build_ids() -> Observable<Vec<String>> {
    bzbus_source::ongoing_ids()
}

pub(in crate::widgets::bar) fn build_view(id: String) -> Observable<Option<BzBusView>> {
    bzbus_source::snapshots()
        .map(move |snapshot| {
            snapshot
                .invocations
                .into_iter()
                .find(|invocation| invocation.id == id)
                .filter(|invocation| snapshot.active && invocation.is_active())
                .map(|invocation| view::view(true, vec![invocation]))
        })
        .distinct_until_changed()
        .box_it()
}
