use std::cell::RefCell;
use std::rc::Rc;

use super::WorkspaceSyncMode;
use super::manager::{
    CommandRunner, ManagerConfig, NiriCommand, PlacementSnapshot, SidecarManager, WindowPlacement,
    WorkspacePlacement,
};

#[test]
fn agent_sidecar_moves_primary_agent_to_same_index_on_sidecar() {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut manager = SidecarManager::new(config(), RecordingRunner(commands.clone()));

    manager
        .apply(&snapshot(WorkspaceSyncMode::AgentSidecar, "primary", 3))
        .unwrap();

    assert_eq!(
        *commands.borrow(),
        vec![
            NiriCommand::MoveWindowToMonitor {
                window_id: 42,
                output: "DP-2".to_owned(),
            },
            NiriCommand::MoveWindowToWorkspace {
                window_id: 42,
                index: 3,
            },
        ]
    );
}

#[test]
fn leaving_agent_sidecar_restores_moved_agent_to_original_workspace() {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut manager = SidecarManager::new(config(), RecordingRunner(commands.clone()));

    manager
        .apply(&snapshot(WorkspaceSyncMode::AgentSidecar, "primary", 3))
        .unwrap();
    commands.borrow_mut().clear();
    manager
        .apply(&snapshot(WorkspaceSyncMode::MirrorBar, "sidecar", 3))
        .unwrap();

    assert_eq!(
        *commands.borrow(),
        vec![
            NiriCommand::MoveWindowToMonitor {
                window_id: 42,
                output: "DP-3".to_owned(),
            },
            NiriCommand::MoveWindowToWorkspace {
                window_id: 42,
                index: 3,
            },
        ]
    );
}

#[test]
fn agent_sidecar_focuses_sidecar_to_main_active_workspace() {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut manager = SidecarManager::new(config(), RecordingRunner(commands.clone()));

    manager
        .apply(&snapshot_with_sidecar_index(
            WorkspaceSyncMode::AgentSidecar,
            "sidecar",
            3,
            1,
        ))
        .unwrap();

    assert_eq!(
        *commands.borrow(),
        vec![
            NiriCommand::FocusMonitor {
                output: "DP-2".to_owned(),
            },
            NiriCommand::FocusWorkspace { index: 3 },
            NiriCommand::FocusMonitor {
                output: "DP-3".to_owned(),
            },
        ]
    );
}

#[test]
fn sidecar_mode_does_not_capture_agents_already_on_sidecar() {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut manager = SidecarManager::new(config(), RecordingRunner(commands.clone()));

    manager
        .apply(&snapshot(WorkspaceSyncMode::AgentSidecar, "sidecar", 3))
        .unwrap();

    assert!(commands.borrow().is_empty());
}

fn config() -> ManagerConfig {
    ManagerConfig {
        primary_output_name: "DP-3".to_owned(),
        primary_output_path: "primary".to_owned(),
        sidecar_output_name: "DP-2".to_owned(),
        sidecar_output_path: "sidecar".to_owned(),
    }
}

fn snapshot(
    mode: WorkspaceSyncMode,
    agent_output_path: &str,
    agent_workspace_index: u32,
) -> PlacementSnapshot {
    let agent_workspace_id = if agent_output_path == "primary" { 1 } else { 2 };
    PlacementSnapshot {
        mode,
        workspaces: vec![
            WorkspacePlacement {
                id: 1,
                index: agent_workspace_index,
                output_path: Some("primary".to_owned()),
                active: true,
                focused: true,
            },
            WorkspacePlacement {
                id: 2,
                index: agent_workspace_index,
                output_path: Some("sidecar".to_owned()),
                active: true,
                focused: false,
            },
        ],
        windows: vec![WindowPlacement {
            id: 42,
            workspace_id: Some(agent_workspace_id),
        }],
        agent_window_ids: vec![42],
    }
}

fn snapshot_with_sidecar_index(
    mode: WorkspaceSyncMode,
    agent_output_path: &str,
    primary_active_index: u32,
    sidecar_active_index: u32,
) -> PlacementSnapshot {
    let agent_workspace_id = if agent_output_path == "primary" { 1 } else { 2 };
    PlacementSnapshot {
        mode,
        workspaces: vec![
            WorkspacePlacement {
                id: 1,
                index: primary_active_index,
                output_path: Some("primary".to_owned()),
                active: true,
                focused: true,
            },
            WorkspacePlacement {
                id: 2,
                index: sidecar_active_index,
                output_path: Some("sidecar".to_owned()),
                active: true,
                focused: false,
            },
        ],
        windows: vec![WindowPlacement {
            id: 42,
            workspace_id: Some(agent_workspace_id),
        }],
        agent_window_ids: vec![42],
    }
}

struct RecordingRunner(Rc<RefCell<Vec<NiriCommand>>>);

impl CommandRunner for RecordingRunner {
    fn run(&mut self, command: &NiriCommand) -> Result<(), String> {
        self.0.borrow_mut().push(command.clone());
        Ok(())
    }
}
