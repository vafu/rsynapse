use std::{collections::HashMap, process::Command};

use futures_util::StreamExt;
use shell_core::source::{self, Observable, rx::Observable as _};
use shell_rx_macros::combine_latest;

use super::{WorkspaceSyncMode, mode};
use crate::widgets::bar::{
    niri::{self, NiriWorkspace},
    window_source::{WindowSnapshot, window_snapshots},
    window_tile::agent::agent_window_ids,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::widgets::bar) struct ManagerStatus {
    pub(super) applied_updates: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ManagerConfig {
    pub(super) primary_output_name: String,
    pub(super) primary_output_path: String,
    pub(super) sidecar_output_name: String,
    pub(super) sidecar_output_path: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct SidecarState {
    originals: HashMap<u64, OriginalPlacement>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OriginalPlacement {
    workspace_index: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct PlacementSnapshot {
    pub(super) mode: WorkspaceSyncMode,
    pub(super) workspaces: Vec<WorkspacePlacement>,
    pub(super) windows: Vec<WindowPlacement>,
    pub(super) agent_window_ids: Vec<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorkspacePlacement {
    pub(super) id: u64,
    pub(super) index: u32,
    pub(super) output_path: Option<String>,
    pub(super) active: bool,
    pub(super) focused: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WindowPlacement {
    pub(super) id: u64,
    pub(super) workspace_id: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum NiriCommand {
    MoveWindowToMonitor { window_id: u64, output: String },
    MoveWindowToWorkspace { window_id: u64, index: u32 },
    FocusMonitor { output: String },
    FocusWorkspace { index: u32 },
}

pub(in crate::widgets::bar) fn manager(
    primary_output_name: Option<String>,
    sidecar_output_name: Option<String>,
) -> Observable<ManagerStatus> {
    let Some(primary_output_name) = primary_output_name else {
        return source::once(ManagerStatus::default());
    };
    let Some(sidecar_output_name) = sidecar_output_name else {
        return source::once(ManagerStatus::default());
    };
    if primary_output_name == sidecar_output_name {
        return source::once(ManagerStatus::default());
    }

    let config = ManagerConfig {
        primary_output_path: niri::output_path_for_name(&primary_output_name),
        sidecar_output_path: niri::output_path_for_name(&sidecar_output_name),
        primary_output_name,
        sidecar_output_name,
    };

    source::from_task(move |sender| {
        let config = config.clone();
        async move {
            run_manager(sender, config, NiriMsgCommandRunner).await;
        }
    })
    .distinct_until_changed()
    .box_it()
}

async fn run_manager(
    sender: async_channel::Sender<Result<ManagerStatus, String>>,
    config: ManagerConfig,
    runner: impl CommandRunner,
) {
    let mut updates = Box::pin(placement_snapshots().into_stream());
    let mut manager = SidecarManager::new(config, runner);
    let mut applied_updates = 0;

    while let Some(update) = updates.next().await {
        match update {
            Ok(snapshot) => {
                if let Err(error) = manager.apply(&snapshot) {
                    eprintln!("[workspace-sync] {error}");
                }
                applied_updates += 1;
                if sender
                    .send(Ok(ManagerStatus { applied_updates }))
                    .await
                    .is_err()
                {
                    return;
                }
            }
            Err(error) => {
                if sender.send(Err(error)).await.is_err() {
                    return;
                }
            }
        }
    }
}

fn placement_snapshots() -> Observable<PlacementSnapshot> {
    combine_latest!(
        mode(),
        source::switch_map_list(niri::workspaces(), workspace_placement),
        window_snapshots().map(window_placements),
        agent_window_ids()
            => |(mode, workspaces, windows, agent_window_ids)| PlacementSnapshot {
                mode,
                workspaces,
                windows,
                agent_window_ids,
            },
    )
    .distinct_until_changed()
    .box_it()
}

fn workspace_placement(workspace: NiriWorkspace) -> Observable<WorkspacePlacement> {
    let id = workspace.path_id().unwrap_or(u64::MAX);
    combine_latest!(
        workspace.index().map(u32::from),
        workspace.output_path_key(),
        workspace.active(),
        workspace.focused()
            => move |(index, output_path, active, focused)| WorkspacePlacement {
                id,
                index,
                output_path,
                active,
                focused,
            },
    )
    .distinct_until_changed()
    .box_it()
}

fn window_placements(windows: Vec<WindowSnapshot>) -> Vec<WindowPlacement> {
    windows
        .into_iter()
        .map(|window| WindowPlacement {
            id: window.id,
            workspace_id: window.workspace_id,
        })
        .collect()
}

pub(super) struct SidecarManager<R> {
    config: ManagerConfig,
    state: SidecarState,
    runner: R,
}

impl<R: CommandRunner> SidecarManager<R> {
    pub(super) fn new(config: ManagerConfig, runner: R) -> Self {
        Self {
            config,
            state: SidecarState::default(),
            runner,
        }
    }

    pub(super) fn apply(&mut self, snapshot: &PlacementSnapshot) -> Result<(), String> {
        let commands = if snapshot.mode == WorkspaceSyncMode::AgentSidecar {
            self.sidecar_commands(snapshot)
        } else {
            self.restore_commands(snapshot)
        };

        for command in &commands {
            self.runner.run(command)?;
        }

        if snapshot.mode != WorkspaceSyncMode::AgentSidecar {
            self.state.originals.clear();
        }

        Ok(())
    }

    fn sidecar_commands(&mut self, snapshot: &PlacementSnapshot) -> Vec<NiriCommand> {
        let view = SnapshotView::new(snapshot);
        let mut commands = self.sidecar_workspace_commands(&view);
        self.state
            .originals
            .retain(|window_id, _| view.window(*window_id).is_some());

        for window_id in &snapshot.agent_window_ids {
            let Some(window) = view.window(*window_id) else {
                continue;
            };
            let Some(workspace) = window.workspace_id.and_then(|id| view.workspace(id)) else {
                continue;
            };

            if workspace.output_path.as_deref() == Some(self.config.primary_output_path.as_str()) {
                self.state
                    .originals
                    .entry(*window_id)
                    .or_insert(OriginalPlacement {
                        workspace_index: workspace.index,
                    });
            }

            let Some(original) = self.state.originals.get(window_id).copied() else {
                continue;
            };

            push_reposition_commands(
                &mut commands,
                *window_id,
                workspace,
                self.config.sidecar_output_path.as_str(),
                self.config.sidecar_output_name.as_str(),
                original.workspace_index,
            );
        }
        commands
    }

    fn sidecar_workspace_commands(&self, view: &SnapshotView<'_>) -> Vec<NiriCommand> {
        let Some(primary_workspace) =
            view.active_workspace(self.config.primary_output_path.as_str())
        else {
            return Vec::new();
        };
        let sidecar_workspace = view.active_workspace(self.config.sidecar_output_path.as_str());
        if sidecar_workspace.map(|workspace| workspace.index) == Some(primary_workspace.index) {
            return Vec::new();
        }

        vec![
            NiriCommand::FocusMonitor {
                output: self.config.sidecar_output_name.clone(),
            },
            NiriCommand::FocusWorkspace {
                index: primary_workspace.index,
            },
            NiriCommand::FocusMonitor {
                output: self.config.primary_output_name.clone(),
            },
        ]
    }

    fn restore_commands(&mut self, snapshot: &PlacementSnapshot) -> Vec<NiriCommand> {
        let view = SnapshotView::new(snapshot);
        self.state
            .originals
            .iter()
            .filter_map(|(window_id, original)| {
                let window = view.window(*window_id)?;
                let workspace = window.workspace_id.and_then(|id| view.workspace(id))?;
                Some((*window_id, *original, workspace))
            })
            .fold(
                Vec::new(),
                |mut commands, (window_id, original, workspace)| {
                    push_reposition_commands(
                        &mut commands,
                        window_id,
                        workspace,
                        self.config.primary_output_path.as_str(),
                        self.config.primary_output_name.as_str(),
                        original.workspace_index,
                    );
                    commands
                },
            )
    }
}

fn push_reposition_commands(
    commands: &mut Vec<NiriCommand>,
    window_id: u64,
    current_workspace: &WorkspacePlacement,
    target_output_path: &str,
    target_output_name: &str,
    target_workspace_index: u32,
) {
    let needs_monitor = current_workspace.output_path.as_deref() != Some(target_output_path);
    let needs_workspace = needs_monitor || current_workspace.index != target_workspace_index;

    if needs_monitor {
        commands.push(NiriCommand::MoveWindowToMonitor {
            window_id,
            output: target_output_name.to_owned(),
        });
    }
    if needs_workspace {
        commands.push(NiriCommand::MoveWindowToWorkspace {
            window_id,
            index: target_workspace_index,
        });
    }
}

struct SnapshotView<'a> {
    workspaces: HashMap<u64, &'a WorkspacePlacement>,
    windows: HashMap<u64, &'a WindowPlacement>,
}

impl<'a> SnapshotView<'a> {
    fn new(snapshot: &'a PlacementSnapshot) -> Self {
        Self {
            workspaces: snapshot
                .workspaces
                .iter()
                .map(|workspace| (workspace.id, workspace))
                .collect(),
            windows: snapshot
                .windows
                .iter()
                .map(|window| (window.id, window))
                .collect(),
        }
    }

    fn workspace(&self, id: u64) -> Option<&'a WorkspacePlacement> {
        self.workspaces.get(&id).copied()
    }

    fn active_workspace(&self, output_path: &str) -> Option<&'a WorkspacePlacement> {
        self.workspaces.values().copied().find(|workspace| {
            workspace.active && workspace.output_path.as_deref() == Some(output_path)
        })
    }

    fn window(&self, id: u64) -> Option<&'a WindowPlacement> {
        self.windows.get(&id).copied()
    }
}

pub(super) trait CommandRunner {
    fn run(&mut self, command: &NiriCommand) -> Result<(), String>;
}

struct NiriMsgCommandRunner;

impl CommandRunner for NiriMsgCommandRunner {
    fn run(&mut self, command: &NiriCommand) -> Result<(), String> {
        let mut process = Command::new("niri");
        match command {
            NiriCommand::MoveWindowToMonitor { window_id, output } => {
                process.args([
                    "msg",
                    "action",
                    "move-window-to-monitor",
                    "--id",
                    window_id.to_string().as_str(),
                    output.as_str(),
                ]);
            }
            NiriCommand::MoveWindowToWorkspace { window_id, index } => {
                process.args([
                    "msg",
                    "action",
                    "move-window-to-workspace",
                    "--window-id",
                    window_id.to_string().as_str(),
                    "--focus",
                    "false",
                    index.to_string().as_str(),
                ]);
            }
            NiriCommand::FocusMonitor { output } => {
                process.args(["msg", "action", "focus-monitor", output.as_str()]);
            }
            NiriCommand::FocusWorkspace { index } => {
                process.args([
                    "msg",
                    "action",
                    "focus-workspace",
                    index.to_string().as_str(),
                ]);
            }
        }

        let output = process
            .output()
            .map_err(|error| format!("spawn niri msg: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "niri msg failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }
}
