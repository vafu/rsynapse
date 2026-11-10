pub(super) fn primary_output(current: Option<&str>, connected: &[String]) -> Option<String> {
    current
        .and_then(|name| connected.iter().find(|output| output.as_str() == name))
        .or_else(|| connected.first())
        .cloned()
}

pub(super) fn secondary_bar_changes(
    existing: &[String],
    connected: &[String],
    primary: &str,
) -> (Vec<String>, Vec<String>) {
    let primary = primary_output(Some(primary), connected);
    let desired = connected
        .iter()
        .filter(|output| Some(output.as_str()) != primary.as_deref())
        .fold(Vec::new(), |mut outputs, output| {
            if !outputs.contains(output) {
                outputs.push(output.clone());
            }
            outputs
        });
    let removed = existing
        .iter()
        .filter(|output| !desired.contains(output))
        .cloned()
        .collect();
    let added = desired
        .into_iter()
        .filter(|output| !existing.contains(output))
        .collect();
    (removed, added)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_output_follows_monitor_removal_and_keeps_a_valid_replacement() {
        let connected = vec!["DP-3".into()];
        assert_eq!(
            primary_output(Some("eDP-1"), &connected),
            Some("DP-3".into())
        );
        let connected = vec!["eDP-1".into(), "DP-3".into()];
        assert_eq!(
            primary_output(Some("DP-3"), &connected),
            Some("DP-3".into())
        );
        assert_eq!(primary_output(Some("DP-3"), &[]), None);
        assert_eq!(primary_output(None, &connected), Some("eDP-1".into()));
    }

    #[test]
    fn replacement_primary_is_removed_from_secondary_controllers() {
        let existing = vec!["DP-3".into()];
        assert_eq!(
            secondary_bar_changes(&existing, &["DP-3".into()], "eDP-1"),
            (vec!["DP-3".into()], vec![])
        );
        assert_eq!(
            secondary_bar_changes(&[], &["eDP-1".into(), "DP-3".into()], "DP-3"),
            (vec![], vec!["eDP-1".into()])
        );
    }
}
