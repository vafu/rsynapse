use shell_core::source::{self, Observable, rx::Observable as _};

use crate::widgets::bar::{
    bzbus::{BzBusView, bzbus_for_window},
    niri::NiriWorkspace,
    window_source::{WindowSnapshot, window_snapshots},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::widgets::bar::project_label) enum WorkspaceBuildState {
    #[default]
    None,
    Running,
    Failed,
    Finished,
}

pub(super) fn workspace_build_state(workspace: NiriWorkspace) -> Observable<WorkspaceBuildState> {
    source::switch_map(
        workspace
            .id()
            .map(Some)
            .combine_latest(window_snapshots(), workspace_windows)
            .distinct_until_changed()
            .box_it(),
        |windows| {
            source::switch_map_list(source::once(windows), window_build_state)
                .map(workspace_build_state_from_builds)
                .distinct_until_changed()
                .box_it()
        },
    )
    .distinct_until_changed()
    .box_it()
}

fn workspace_windows(
    workspace_id: Option<u64>,
    windows: Vec<WindowSnapshot>,
) -> Vec<WindowSnapshot> {
    windows
        .into_iter()
        .filter(|window| window.workspace_id == workspace_id)
        .collect()
}

fn window_build_state(window: WindowSnapshot) -> Observable<Option<BzBusView>> {
    bzbus_for_window(window.window)
}

pub(super) fn workspace_build_state_from_builds(
    builds: Vec<Option<BzBusView>>,
) -> WorkspaceBuildState {
    let builds = builds.iter().flatten().collect::<Vec<_>>();
    if builds.is_empty() {
        return WorkspaceBuildState::None;
    }
    if builds.iter().any(|build| has_state(build, "failed")) {
        return WorkspaceBuildState::Failed;
    }
    if builds.iter().any(|build| has_state(build, "running")) {
        return WorkspaceBuildState::Running;
    }
    if builds.iter().all(|build| has_state(build, "finished")) {
        return WorkspaceBuildState::Finished;
    }
    WorkspaceBuildState::None
}

fn has_state(build: &BzBusView, state: &str) -> bool {
    build.classes.iter().any(|class| class == &state)
}
