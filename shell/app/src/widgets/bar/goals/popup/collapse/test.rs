use super::*;
use shell_core::source::proj::GoalInfo;
fn row(id: &str, depth: usize, children: usize, completed: bool) -> GoalRow {
    GoalRow {
        goal: GoalInfo {
            id: id.into(),
            ..Default::default()
        },
        depth,
        completed,
        child_count: children,
        completed_children: if completed { children } else { 0 },
        instance_id: id.into(),
        created_at_ms: 0,
    }
}

#[test]
fn folds_hide_whole_descendant_spans_and_can_be_manually_reopened() {
    let rows = vec![
        row("root", 0, 1, true),
        row("child", 1, 1, false),
        row("nested", 2, 0, false),
        row("other", 0, 0, false),
    ];
    let mut folds = CollapseState::default();
    folds.sync(&rows);
    assert_eq!(folds.visibility(&rows), vec![true, false, false, true]);
    folds.toggle(&rows[0]);
    assert_eq!(folds.visibility(&rows), vec![true, true, true, true]);
    folds.toggle(&rows[1]);
    assert_eq!(folds.visibility(&rows), vec![true, true, false, true]);
    folds.sync(&rows);
    assert_eq!(folds.visibility(&rows), vec![true, true, false, true]);
}
#[test]
fn completion_transitions_collapse_and_reopening_restores_children() {
    let mut rows = vec![row("root", 0, 1, false), row("child", 1, 0, false)];
    let mut folds = CollapseState::default();
    folds.sync(&rows);
    assert_eq!(folds.visibility(&rows), vec![true, true]);
    rows[0].completed = true;
    folds.sync(&rows);
    assert_eq!(folds.visibility(&rows), vec![true, false]);
    folds.toggle(&rows[0]);
    folds.sync(&rows);
    assert_eq!(folds.visibility(&rows), vec![true, true]);
    rows[0].completed = false;
    folds.sync(&rows);
    assert_eq!(folds.visibility(&rows), vec![true, true]);
}
