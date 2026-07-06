use std::{
    collections::{HashMap, HashSet},
    panic::{AssertUnwindSafe, catch_unwind},
};

use niri_ipc::{
    Event, KeyboardLayouts, Output, Window, Workspace,
    state::{EventStreamState, EventStreamStatePart},
};
use zbus::zvariant::OwnedObjectPath;

use niri_dbus::paths;

#[derive(Debug, Default)]
pub struct NiriState {
    pub connected: bool,
    pub compositor_version: String,
    pub generation: u64,
    pub outputs: HashMap<String, Output>,
    pub event_state: EventStreamState,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ObjectSet {
    pub outputs: HashSet<String>,
    pub workspaces: HashSet<u64>,
    pub windows: HashSet<u64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ObjectDelta {
    pub added: ObjectSet,
    pub removed: ObjectSet,
}

impl NiriState {
    pub fn mark_connected(
        &mut self,
        version: String,
        outputs: HashMap<String, Output>,
    ) -> ObjectDelta {
        let before = self.object_set();
        self.connected = true;
        self.compositor_version = version;
        self.outputs = outputs;
        self.event_state = EventStreamState::default();
        self.generation = self.generation.wrapping_add(1);
        self.object_set().delta_from(&before)
    }

    pub fn mark_disconnected(&mut self) -> ObjectDelta {
        let before = self.object_set();
        let had_projected_state = self.connected
            || !before.outputs.is_empty()
            || !before.workspaces.is_empty()
            || !before.windows.is_empty();
        self.connected = false;
        self.outputs.clear();
        self.event_state = EventStreamState::default();
        if had_projected_state {
            self.generation = self.generation.wrapping_add(1);
        }
        self.object_set().delta_from(&before)
    }

    pub fn apply_event(&mut self, event: Event) -> anyhow::Result<ObjectDelta> {
        let before = self.object_set();
        let result = catch_unwind(AssertUnwindSafe(|| self.event_state.apply(event)));
        if result.is_err() {
            anyhow::bail!("niri EventStreamState rejected event ordering");
        }
        Ok(self.object_set().delta_from(&before))
    }

    pub fn object_set(&self) -> ObjectSet {
        ObjectSet {
            outputs: self.outputs.keys().cloned().collect(),
            workspaces: self
                .event_state
                .workspaces
                .workspaces
                .keys()
                .copied()
                .collect(),
            windows: self.event_state.windows.windows.keys().copied().collect(),
        }
    }

    pub fn output(&self, name: &str) -> Option<&Output> {
        self.outputs.get(name)
    }

    pub fn workspace(&self, id: u64) -> Option<&Workspace> {
        self.event_state.workspaces.workspaces.get(&id)
    }

    pub fn window(&self, id: u64) -> Option<&Window> {
        self.event_state.windows.windows.get(&id)
    }

    pub fn keyboard_layouts(&self) -> Option<&KeyboardLayouts> {
        self.event_state.keyboard_layouts.keyboard_layouts.as_ref()
    }

    pub fn overview_open(&self) -> bool {
        self.event_state.overview.is_open
    }

    pub fn config_load_failed(&self) -> bool {
        self.event_state.config.failed
    }

    pub fn focused_workspace_id(&self) -> Option<u64> {
        self.event_state
            .workspaces
            .workspaces
            .values()
            .find(|workspace| workspace.is_focused)
            .map(|workspace| workspace.id)
    }

    pub fn focused_workspace_path(&self) -> Option<OwnedObjectPath> {
        self.focused_workspace_id().map(paths::workspace_path)
    }

    pub fn output_paths(&self) -> Vec<OwnedObjectPath> {
        let mut names = self.outputs.keys().collect::<Vec<_>>();
        names.sort();
        names
            .into_iter()
            .map(|name| paths::output_path(name))
            .collect()
    }

    pub fn workspace_paths(&self) -> Vec<OwnedObjectPath> {
        self.sorted_workspaces()
            .into_iter()
            .map(|workspace| paths::workspace_path(workspace.id))
            .collect()
    }

    pub fn window_paths(&self) -> Vec<OwnedObjectPath> {
        self.sorted_windows()
            .into_iter()
            .map(|window| paths::window_path(window.id))
            .collect()
    }

    pub fn focused_window_id(&self) -> Option<u64> {
        self.event_state
            .windows
            .windows
            .values()
            .find(|window| window.is_focused)
            .map(|window| window.id)
    }

    pub fn focused_window_path(&self) -> Option<OwnedObjectPath> {
        self.focused_window_id().map(paths::window_path)
    }

    pub fn focused_output_name(&self) -> Option<String> {
        self.event_state
            .workspaces
            .workspaces
            .values()
            .find(|workspace| workspace.is_focused)
            .and_then(|workspace| workspace.output.clone())
    }

    pub fn focused_output_path(&self) -> Option<OwnedObjectPath> {
        self.focused_output_name()
            .map(|name| paths::output_path(&name))
    }

    pub fn current_workspace_for_output(&self, output_name: &str) -> Option<OwnedObjectPath> {
        self.event_state
            .workspaces
            .workspaces
            .values()
            .find(|workspace| {
                workspace.is_active && workspace.output.as_deref() == Some(output_name)
            })
            .map(|workspace| paths::workspace_path(workspace.id))
    }

    pub fn workspaces_for_output(&self, output_name: &str) -> Vec<OwnedObjectPath> {
        self.sorted_workspaces()
            .into_iter()
            .filter(|workspace| workspace.output.as_deref() == Some(output_name))
            .map(|workspace| paths::workspace_path(workspace.id))
            .collect()
    }

    pub fn windows_for_workspace(&self, workspace_id: u64) -> Vec<OwnedObjectPath> {
        self.sorted_windows()
            .into_iter()
            .filter(|window| window.workspace_id == Some(workspace_id))
            .map(|window| paths::window_path(window.id))
            .collect()
    }

    pub fn output_for_window(&self, window: &Window) -> Option<OwnedObjectPath> {
        let workspace_id = window.workspace_id?;
        let workspace = self.workspace(workspace_id)?;
        workspace
            .output
            .as_ref()
            .map(|output| paths::output_path(output))
    }

    fn sorted_workspaces(&self) -> Vec<&Workspace> {
        let mut workspaces = self
            .event_state
            .workspaces
            .workspaces
            .values()
            .collect::<Vec<_>>();
        workspaces.sort_by(|left, right| {
            left.output
                .cmp(&right.output)
                .then_with(|| left.idx.cmp(&right.idx))
                .then_with(|| left.id.cmp(&right.id))
        });
        workspaces
    }

    fn sorted_windows(&self) -> Vec<&Window> {
        let mut windows = self
            .event_state
            .windows
            .windows
            .values()
            .collect::<Vec<_>>();
        windows.sort_by(|left, right| {
            left.workspace_id
                .cmp(&right.workspace_id)
                .then_with(|| window_layout_key(left).cmp(&window_layout_key(right)))
                .then_with(|| left.id.cmp(&right.id))
        });
        windows
    }
}

fn window_layout_key(window: &Window) -> (usize, usize) {
    window
        .layout
        .pos_in_scrolling_layout
        .unwrap_or((usize::MAX, usize::MAX))
}

impl ObjectSet {
    fn delta_from(&self, before: &Self) -> ObjectDelta {
        ObjectDelta {
            added: ObjectSet {
                outputs: self.outputs.difference(&before.outputs).cloned().collect(),
                workspaces: self
                    .workspaces
                    .difference(&before.workspaces)
                    .copied()
                    .collect(),
                windows: self.windows.difference(&before.windows).copied().collect(),
            },
            removed: ObjectSet {
                outputs: before.outputs.difference(&self.outputs).cloned().collect(),
                workspaces: before
                    .workspaces
                    .difference(&self.workspaces)
                    .copied()
                    .collect(),
                windows: before.windows.difference(&self.windows).copied().collect(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use niri_ipc::{
        Event, LogicalOutput, Mode, Output, Timestamp, Transform, Window, WindowLayout, Workspace,
    };

    use super::*;

    #[test]
    fn applies_workspace_events_into_focus_paths() {
        let mut state = NiriState::default();
        state
            .apply_event(Event::WorkspacesChanged {
                workspaces: vec![Workspace {
                    id: 5,
                    idx: 1,
                    name: Some("dev".to_owned()),
                    output: Some("eDP-1".to_owned()),
                    is_urgent: false,
                    is_active: true,
                    is_focused: true,
                    active_window_id: None,
                }],
            })
            .expect("event applies");

        assert_eq!(
            state.focused_workspace_path().unwrap().as_str(),
            "/org/rsynapse/Niri/Workspaces/workspace_5"
        );
        assert_eq!(
            state.focused_output_path().unwrap().as_str(),
            "/org/rsynapse/Niri/Outputs/x6544502D31"
        );
    }

    #[test]
    fn object_delta_tracks_output_workspace_window_lifecycle() {
        let mut state = NiriState::default();
        let delta = state.mark_connected(
            "niri 26.4".to_owned(),
            HashMap::from([("eDP-1".to_owned(), output("eDP-1"))]),
        );
        assert_eq!(delta.added.outputs, HashSet::from(["eDP-1".to_owned()]));

        let delta = state
            .apply_event(Event::WorkspacesChanged {
                workspaces: vec![
                    workspace(5, 1, "eDP-1", true, true, None),
                    workspace(6, 2, "eDP-1", false, false, None),
                ],
            })
            .expect("workspaces apply");
        assert_eq!(delta.added.workspaces, HashSet::from([5, 6]));

        let delta = state
            .apply_event(Event::WindowsChanged {
                windows: vec![
                    window(20, Some(5), Some((2, 1)), false),
                    window(10, Some(5), Some((1, 1)), true),
                ],
            })
            .expect("windows apply");
        assert_eq!(delta.added.windows, HashSet::from([10, 20]));

        let delta = state
            .apply_event(Event::WindowsChanged {
                windows: vec![window(20, Some(5), Some((2, 1)), false)],
            })
            .expect("windows replace applies");
        assert_eq!(delta.removed.windows, HashSet::from([10]));

        let delta = state.mark_disconnected();
        assert_eq!(delta.removed.outputs, HashSet::from(["eDP-1".to_owned()]));
        assert_eq!(delta.removed.workspaces, HashSet::from([5, 6]));
        assert_eq!(delta.removed.windows, HashSet::from([20]));
        assert!(!state.connected);
    }

    #[test]
    fn generation_changes_on_connect_and_meaningful_disconnect_only() {
        let mut state = NiriState::default();

        state.mark_disconnected();
        assert_eq!(state.generation, 0);

        state.mark_connected(
            "niri 26.4".to_owned(),
            HashMap::from([("eDP-1".to_owned(), output("eDP-1"))]),
        );
        assert_eq!(state.generation, 1);

        state.mark_disconnected();
        assert_eq!(state.generation, 2);

        state.mark_disconnected();
        assert_eq!(state.generation, 2);
    }

    #[test]
    fn relationship_paths_are_stably_sorted() {
        let mut state = NiriState::default();
        state.mark_connected(
            "niri 26.4".to_owned(),
            HashMap::from([
                ("HDMI-A-1".to_owned(), output("HDMI-A-1")),
                ("eDP-1".to_owned(), output("eDP-1")),
            ]),
        );
        state
            .apply_event(Event::WorkspacesChanged {
                workspaces: vec![
                    workspace(30, 3, "eDP-1", false, false, None),
                    workspace(10, 1, "eDP-1", true, true, Some(200)),
                    workspace(20, 1, "HDMI-A-1", true, false, None),
                ],
            })
            .expect("workspaces apply");
        state
            .apply_event(Event::WindowsChanged {
                windows: vec![
                    window(300, Some(10), Some((2, 1)), false),
                    window(100, Some(10), Some((1, 2)), false),
                    window(200, Some(10), Some((1, 1)), true),
                ],
            })
            .expect("windows apply");

        assert_eq!(
            state
                .output_paths()
                .into_iter()
                .map(path_string)
                .collect::<Vec<_>>(),
            vec![
                "/org/rsynapse/Niri/Outputs/x48444D492D412D31",
                "/org/rsynapse/Niri/Outputs/x6544502D31"
            ]
        );
        assert_eq!(
            state
                .workspaces_for_output("eDP-1")
                .into_iter()
                .map(path_string)
                .collect::<Vec<_>>(),
            vec![
                "/org/rsynapse/Niri/Workspaces/workspace_10",
                "/org/rsynapse/Niri/Workspaces/workspace_30"
            ]
        );
        assert_eq!(
            state
                .windows_for_workspace(10)
                .into_iter()
                .map(path_string)
                .collect::<Vec<_>>(),
            vec![
                "/org/rsynapse/Niri/Windows/window_200",
                "/org/rsynapse/Niri/Windows/window_100",
                "/org/rsynapse/Niri/Windows/window_300"
            ]
        );
    }

    #[test]
    fn focus_paths_follow_focus_events() {
        let mut state = NiriState::default();
        state
            .apply_event(Event::WindowsChanged {
                windows: vec![
                    window(1, Some(5), Some((1, 1)), true),
                    window(2, Some(5), Some((1, 2)), false),
                ],
            })
            .expect("windows apply");

        assert_eq!(
            state.focused_window_path().unwrap().as_str(),
            "/org/rsynapse/Niri/Windows/window_1"
        );

        state
            .apply_event(Event::WindowFocusChanged { id: Some(2) })
            .expect("focus applies");
        assert_eq!(
            state.focused_window_path().unwrap().as_str(),
            "/org/rsynapse/Niri/Windows/window_2"
        );

        state
            .apply_event(Event::WindowFocusChanged { id: None })
            .expect("focus clears");
        assert_eq!(state.focused_window_path(), None);
    }

    fn output(name: &str) -> Output {
        Output {
            name: name.to_owned(),
            make: "Acme".to_owned(),
            model: "Panel".to_owned(),
            serial: Some("serial".to_owned()),
            physical_size: Some((300, 200)),
            modes: vec![Mode {
                width: 1920,
                height: 1080,
                refresh_rate: 60_000,
                is_preferred: true,
            }],
            current_mode: Some(0),
            is_custom_mode: false,
            vrr_supported: true,
            vrr_enabled: false,
            logical: Some(LogicalOutput {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
                scale: 1.0,
                transform: Transform::Normal,
            }),
        }
    }

    fn workspace(
        id: u64,
        idx: u8,
        output: &str,
        is_active: bool,
        is_focused: bool,
        active_window_id: Option<u64>,
    ) -> Workspace {
        Workspace {
            id,
            idx,
            name: None,
            output: Some(output.to_owned()),
            is_urgent: false,
            is_active,
            is_focused,
            active_window_id,
        }
    }

    fn window(
        id: u64,
        workspace_id: Option<u64>,
        pos_in_scrolling_layout: Option<(usize, usize)>,
        is_focused: bool,
    ) -> Window {
        Window {
            id,
            title: Some(format!("window {id}")),
            app_id: Some("test-app".to_owned()),
            pid: Some(1000 + id as i32),
            workspace_id,
            is_focused,
            is_floating: false,
            is_urgent: false,
            layout: WindowLayout {
                pos_in_scrolling_layout,
                tile_size: (800.0, 600.0),
                window_size: (780, 580),
                tile_pos_in_workspace_view: Some((10.0, 20.0)),
                window_offset_in_tile: (5.0, 6.0),
            },
            focus_timestamp: Some(Timestamp {
                secs: 1,
                nanos: id as u32,
            }),
        }
    }

    fn path_string(path: OwnedObjectPath) -> String {
        path.as_str().to_owned()
    }
}
