use super::protocol::{Event, Layer, Output, Window, Workspace};
use crate::model::{self, Projection};
use std::collections::BTreeMap;
use workspace_dbus_api::{output_path, window_path, workspace_path};

#[derive(Default)]
pub(super) struct State {
    outputs: Option<BTreeMap<String, Output>>,
    workspaces: Option<BTreeMap<String, Workspace>>,
    windows: Option<BTreeMap<String, Window>>,
    layers: Option<Vec<Layer>>,
    overview: Option<bool>,
}

impl State {
    pub fn apply(&mut self, event: Event) -> anyhow::Result<Option<Projection>> {
        match event {
            Event::Outputs(values) => self.outputs = Some(keyed(values, |v| &v.name)?),
            Event::Workspaces(values) => {
                anyhow::ensure!(
                    values.iter().all(|w| w.index > 0),
                    "Umbriel workspace indices must be one-based"
                );
                self.workspaces = Some(keyed(values, |v| &v.id)?);
            }
            Event::Windows(values) => self.windows = Some(keyed(values, |v| &v.id)?),
            Event::Layers(values) => self.layers = Some(values),
            Event::Overview(state) => self.overview = Some(state.open),
            Event::Unknown => return Ok(None),
        }
        let (Some(outputs), Some(workspaces), Some(windows), Some(layers), Some(overview)) = (
            &self.outputs,
            &self.workspaces,
            &self.windows,
            &self.layers,
            &self.overview,
        ) else {
            return Ok(None);
        };
        Ok(Some(project(
            outputs, workspaces, windows, layers, *overview,
        )))
    }
}

fn keyed<T>(values: Vec<T>, key: impl Fn(&T) -> &str) -> anyhow::Result<BTreeMap<String, T>> {
    let mut result = BTreeMap::new();
    for value in values {
        let id = key(&value).to_owned();
        anyhow::ensure!(!id.is_empty(), "Umbriel sent an empty object identity");
        anyhow::ensure!(
            result.insert(id.clone(), value).is_none(),
            "Umbriel sent duplicate identity {id:?}"
        );
    }
    Ok(result)
}

fn active_workspace<'a>(
    output: &Output,
    workspaces: &'a BTreeMap<String, Workspace>,
) -> Option<&'a Workspace> {
    workspaces
        .get(&output.active_workspace)
        .filter(|w| w.active && w.output == output.name)
        .or_else(|| {
            workspaces
                .values()
                .find(|w| w.active && w.output == output.name)
        })
}

fn project(
    outputs: &BTreeMap<String, Output>,
    workspaces: &BTreeMap<String, Workspace>,
    windows: &BTreeMap<String, Window>,
    layers: &[Layer],
    overview: bool,
) -> Projection {
    let layer = layers.iter().find(|l| l.mapped && l.focused);
    // Workspace-local `focused` is deliberately not deserialized for windows.
    // Layers can own the keyboard while application activation chrome persists.
    let focused_window = if layer.is_none() && !overview {
        windows.values().find(|w| w.active)
    } else {
        None
    };
    let focused_output = if let Some(window) = focused_window {
        outputs.get(&window.output)
    } else {
        layer
            .and_then(|layer| outputs.get(&layer.output))
            .or_else(|| outputs.values().find(|o| o.focused))
            .or_else(|| {
                workspaces
                    .values()
                    .find(|w| w.focused)
                    .and_then(|w| outputs.get(&w.output))
            })
    };
    let focused_workspace = if let Some(window) = focused_window {
        workspaces.get(&window.workspace)
    } else {
        focused_output.and_then(|output| active_workspace(output, workspaces))
    };
    let mut result = Projection::default();
    result.manager.connected = true;
    result.manager.focused_window = focused_window
        .map(|w| window_path(&w.id))
        .into_iter()
        .collect();
    result.manager.focused_output = focused_output
        .map(|o| output_path(&o.name))
        .into_iter()
        .collect();
    result.manager.focused_workspace = focused_workspace
        .map(|w| workspace_path(&w.id))
        .into_iter()
        .collect();
    for output in outputs.values() {
        let mut members: Vec<_> = workspaces
            .values()
            .filter(|w| w.output == output.name)
            .collect();
        members.sort_by(|a, b| (a.index, &a.id).cmp(&(b.index, &b.id)));
        result.outputs.insert(
            output.name.clone(),
            model::Output {
                focused: focused_output.is_some_and(|o| o.name == output.name),
                current_workspace: active_workspace(output, workspaces)
                    .map(|w| workspace_path(&w.id))
                    .into_iter()
                    .collect(),
                workspaces: members.iter().map(|w| workspace_path(&w.id)).collect(),
            },
        );
    }
    for workspace in workspaces.values() {
        let mut members: Vec<_> = windows
            .values()
            .filter(|w| w.workspace == workspace.id)
            .collect();
        members.sort_by(|a, b| {
            (position(a).unwrap_or((u64::MAX, u64::MAX)), &a.id)
                .cmp(&(position(b).unwrap_or((u64::MAX, u64::MAX)), &b.id))
        });
        result.workspaces.insert(
            workspace.id.clone(),
            model::Workspace {
                layout: workspace.layout,
                name: workspace
                    .named
                    .then(|| workspace.name.clone())
                    .into_iter()
                    .collect(),
                index: workspace.index,
                output: (!workspace.output.is_empty())
                    .then(|| output_path(&workspace.output))
                    .into_iter()
                    .collect(),
                active: workspace.active,
                focused: focused_workspace.is_some_and(|w| w.id == workspace.id),
                urgent: workspace.urgent,
                active_window: windows
                    .get(&workspace.focused_window)
                    .filter(|w| w.workspace == workspace.id)
                    .map(|w| window_path(&w.id))
                    .into_iter()
                    .collect(),
                windows: members.iter().map(|w| window_path(&w.id)).collect(),
            },
        );
    }
    for window in windows.values() {
        let position = position(window);
        result.windows.insert(
            window.id.clone(),
            model::Window {
                title: (!window.title.is_empty())
                    .then(|| window.title.clone())
                    .into_iter()
                    .collect(),
                app_id: (!window.app_id.is_empty())
                    .then(|| window.app_id.clone())
                    .into_iter()
                    .collect(),
                pid: (window.pid > 0).then_some(window.pid).into_iter().collect(),
                workspace: (!window.workspace.is_empty())
                    .then(|| workspace_path(&window.workspace))
                    .into_iter()
                    .collect(),
                output: (!window.output.is_empty())
                    .then(|| output_path(&window.output))
                    .into_iter()
                    .collect(),
                focused: focused_window.is_some_and(|w| w.id == window.id),
                floating: window.floating,
                urgent: window.urgent,
                column_index: position.map(|p| p.0).into_iter().collect(),
                row_index: position.map(|p| p.1).into_iter().collect(),
            },
        );
    }
    result.normalize();
    result
}

fn position(window: &Window) -> Option<(u64, u64)> {
    if window.floating {
        return None;
    }
    Some((
        u64::try_from(window.column).ok()?,
        u64::try_from(window.row).ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bootstrap_requires_observed_focus_state_not_default_assumptions() {
        let mut state = State::default();
        for event in [
            Event::Outputs(vec![]),
            Event::Workspaces(vec![]),
            Event::Windows(vec![]),
            Event::Layers(vec![]),
        ] {
            assert!(state.apply(event).unwrap().is_none());
        }
        let event = serde_json::from_str(r#"{"event":"overview","data":{"open":false}}"#).unwrap();
        assert!(state.apply(event).unwrap().unwrap().manager.connected);
    }
}
