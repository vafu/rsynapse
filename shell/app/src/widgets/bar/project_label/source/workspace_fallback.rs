use shell_core::{source::Observable, source::rx::Observable as _};

use super::non_empty;
use crate::{
    desktop_icon,
    widgets::bar::niri::NiriWorkspace,
    widgets::bar::window_source::{WindowSnapshot, window_snapshots},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct WorkspaceFallback {
    pub(super) icon: Option<String>,
    pub(super) empty: bool,
}

pub(super) fn workspace_window_fallback_source(
    workspace: NiriWorkspace,
) -> Observable<WorkspaceFallback> {
    let workspace_id = workspace.id().map(Some);

    workspace_id
        .combine_latest(window_snapshots(), workspace_window_fallback)
        .distinct_until_changed()
        .box_it()
}

fn workspace_window_fallback(
    workspace_id: Option<u64>,
    mut windows: Vec<WindowSnapshot>,
) -> WorkspaceFallback {
    let Some(workspace_id) = workspace_id else {
        return WorkspaceFallback {
            icon: None,
            empty: true,
        };
    };

    windows.retain(|window| window.workspace_id == Some(workspace_id));
    windows.sort_by(|left, right| {
        (left.column, left.row, left.id)
            .cmp(&(right.column, right.row, right.id))
            .then_with(|| left.window.path_key().cmp(right.window.path_key()))
    });
    let empty = windows.is_empty();

    WorkspaceFallback {
        icon: windows
            .into_iter()
            .filter_map(|window| window.app_id)
            .find_map(|app_id| app_icon(&app_id)),
        empty,
    }
}

fn app_icon(app_id: &str) -> Option<String> {
    let desktop_icon = non_empty(Some(desktop_icon::icon_for_app_id(app_id)));
    desktop_icon.or_else(|| non_empty(Some(app_id.to_owned())))
}
