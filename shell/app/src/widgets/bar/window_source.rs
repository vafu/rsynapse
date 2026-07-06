use shell_core::source::{self, Observable, rx::Observable as _};
use shell_rx_macros::combine_latest;

use super::niri::{self, NiriWindow};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::widgets::bar) struct WindowSnapshot {
    pub(in crate::widgets::bar) window: NiriWindow,
    pub(in crate::widgets::bar) workspace_id: Option<u64>,
    pub(in crate::widgets::bar) column: u64,
    pub(in crate::widgets::bar) row: u64,
    pub(in crate::widgets::bar) id: u64,
    pub(in crate::widgets::bar) app_id: Option<String>,
}

pub(in crate::widgets::bar) fn window_snapshots() -> Observable<Vec<WindowSnapshot>> {
    source::shared_by_key("rsynapse.window-snapshots", "all", || {
        source::switch_map_list(niri::windows(), window_snapshot)
            .distinct_until_changed()
            .box_it()
    })
}

fn window_snapshot(window: NiriWindow) -> Observable<WindowSnapshot> {
    combine_latest!(
        window.workspace().switch_map(|workspace| {
            workspace
                .map(|workspace| workspace.id().map(Some).box_it())
                .unwrap_or_else(|| source::once(None))
        }),
        window.column_index().map(|column| column.unwrap_or(u64::MAX)),
        window.row_index().map(|row| row.unwrap_or(u64::MAX)),
        window.id(),
        window.app_id().map(|app_id| app_id.and_then(non_empty))
            => move |(workspace_id, column, row, id, app_id)| WindowSnapshot {
                window: window.clone(),
                workspace_id,
                column,
                row,
                id,
                app_id,
            },
    )
    .distinct_until_changed()
    .box_it()
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}
