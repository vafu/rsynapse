//! Full snapshot families can arrive in any order. Native occupancy counts and
//! references delimit a coherent topology without a timer or debounce.
use super::{Output, Window, Workspace};
use std::collections::BTreeMap;

pub(super) fn consistent(
    outputs: &BTreeMap<String, Output>,
    workspaces: &BTreeMap<String, Workspace>,
    windows: &BTreeMap<String, Window>,
) -> bool {
    workspaces.values().all(|w| outputs.contains_key(&w.output))
        && windows.values().all(|w| {
            w.workspace.is_empty()
                || workspaces
                    .get(&w.workspace)
                    .and_then(|ws| outputs.get(&ws.output))
                    .is_some_and(|output| output.enabled)
        })
        && workspaces.values().all(|workspace| {
            workspace.window_count.is_none_or(|count| {
                windows
                    .values()
                    .filter(|window| window.workspace == workspace.id)
                    .count()
                    == count
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn output() -> BTreeMap<String, Output> {
        BTreeMap::from([(
            "screen".into(),
            serde_json::from_value(json!({
            "name":"screen", "focused":true, "active_workspace":"a"}))
            .unwrap(),
        )])
    }
    fn workspace(id: &str, count: usize) -> Workspace {
        serde_json::from_value(json!({"id":id,"name":"1","named":false,"index":1,"output":"screen",
            "active":true,"focused":true,"urgent":false,"focused_window":"window","window_count":count})).unwrap()
    }
    fn window(home: &str) -> BTreeMap<String, Window> {
        BTreeMap::from([(
            "window".into(),
            serde_json::from_value(json!({"id":"window","title":"test",
            "app_id":"test","pid":1,"workspace":home,"output":"screen","active":true,
            "floating":false,"urgent":false,"column":0,"row":0}))
            .unwrap(),
        )])
    }
    #[test]
    fn window_first_move_waits_for_workspace_counts() {
        let old = BTreeMap::from([
            ("a".into(), workspace("a", 1)),
            ("b".into(), workspace("b", 0)),
        ]);
        assert!(consistent(&output(), &old, &window("a")));
        assert!(!consistent(&output(), &old, &window("b")));
        let new = BTreeMap::from([
            ("a".into(), workspace("a", 0)),
            ("b".into(), workspace("b", 1)),
        ]);
        assert!(consistent(&output(), &new, &window("b")));
    }
    #[test]
    fn removed_workspace_and_output_families_cannot_leak_partial_topology() {
        let workspaces = BTreeMap::from([("a".into(), workspace("a", 1))]);
        assert!(!consistent(&BTreeMap::new(), &workspaces, &window("a")));
        assert!(!consistent(&output(), &BTreeMap::new(), &window("a")));
        assert!(!consistent(&output(), &workspaces, &BTreeMap::new()));
        assert!(consistent(&output(), &BTreeMap::new(), &window("")));
    }
    #[test]
    fn disablement_waits_until_mapped_windows_leave_the_disabled_output() {
        let mut disabled = output();
        disabled.get_mut("screen").unwrap().enabled = false;
        assert!(!consistent(
            &disabled,
            &BTreeMap::from([("a".into(), workspace("a", 1))]),
            &window("a")
        ));
        assert!(consistent(
            &disabled,
            &BTreeMap::from([("a".into(), workspace("a", 0))]),
            &window("")
        ));
    }
    #[test]
    fn reducer_keeps_last_complete_projection_until_family_counts_agree() {
        use crate::backend::umbriel::protocol::Event;
        let mut state = super::super::State::default();
        for event in [
            Event::Outputs(output().into_values().collect()),
            Event::Workspaces(vec![workspace("a", 1), workspace("b", 0)]),
            Event::Windows(window("a").into_values().collect()),
            Event::Layers(vec![]),
        ] {
            assert!(state.apply(event).unwrap().is_none());
        }
        let initial = state
            .apply(Event::Overview(
                serde_json::from_value(json!({"open":false})).unwrap(),
            ))
            .unwrap()
            .unwrap();
        assert_eq!(
            initial.windows["window"].workspace,
            vec![workspace_dbus_api::workspace_path("a")]
        );
        assert!(
            state
                .apply(Event::Windows(window("b").into_values().collect()))
                .unwrap()
                .is_none()
        );
        let moved = state
            .apply(Event::Workspaces(vec![
                workspace("a", 0),
                workspace("b", 1),
            ]))
            .unwrap()
            .unwrap();
        assert_eq!(
            moved.windows["window"].workspace,
            vec![workspace_dbus_api::workspace_path("b")]
        );
    }
}
