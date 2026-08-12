mod manager;

#[cfg(test)]
mod test;

pub(super) use manager::{ManagerStatus, manager};

use std::sync::OnceLock;

use shell_core::source::{Observable, StateSignal};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum WorkspaceSyncMode {
    #[default]
    Off,
    MirrorBar,
    AgentSidecar,
}

impl WorkspaceSyncMode {
    pub(super) fn next(self) -> Self {
        match self {
            Self::Off => Self::MirrorBar,
            Self::MirrorBar => Self::AgentSidecar,
            Self::AgentSidecar => Self::Off,
        }
    }

    pub(super) fn mirrors_workspace_rail(self) -> bool {
        !matches!(self, Self::Off)
    }
}

pub(super) fn mode() -> Observable<WorkspaceSyncMode> {
    signal().observable()
}

pub(super) fn cycle() {
    signal().update(|mode| *mode = mode.next());
}

pub(super) fn set(mode: WorkspaceSyncMode) {
    signal().set(mode);
}

pub(super) fn corner_classes(mode: WorkspaceSyncMode) -> Vec<&'static str> {
    let mut classes = vec!["flat", "bar-indicator", "bar-corner-indicator"];
    match mode {
        WorkspaceSyncMode::Off => {}
        WorkspaceSyncMode::MirrorBar => classes.push("workspace-sync-mirror"),
        WorkspaceSyncMode::AgentSidecar => classes.push("workspace-sync-agent"),
    }
    classes
}

pub(super) fn tooltip(mode: WorkspaceSyncMode) -> &'static str {
    match mode {
        WorkspaceSyncMode::Off => "Workspace sync: off",
        WorkspaceSyncMode::MirrorBar => "Workspace sync: mirror workspace rails",
        WorkspaceSyncMode::AgentSidecar => "Workspace sync: agent sidecar",
    }
}

fn signal() -> &'static StateSignal<WorkspaceSyncMode> {
    static SIGNAL: OnceLock<StateSignal<WorkspaceSyncMode>> = OnceLock::new();
    SIGNAL.get_or_init(|| StateSignal::new(WorkspaceSyncMode::Off))
}

#[cfg(test)]
mod tests {
    use super::WorkspaceSyncMode;

    #[test]
    fn workspace_sync_modes_cycle() {
        assert_eq!(WorkspaceSyncMode::Off.next(), WorkspaceSyncMode::MirrorBar);
        assert_eq!(
            WorkspaceSyncMode::MirrorBar.next(),
            WorkspaceSyncMode::AgentSidecar
        );
        assert_eq!(
            WorkspaceSyncMode::AgentSidecar.next(),
            WorkspaceSyncMode::Off
        );
    }
}
