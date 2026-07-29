use super::selected_project_view;
use crate::widgets::bar::project::ProjectDetails;

#[test]
fn selected_project_displays_project_secondary() {
    let view = selected_project_view(3, "", split_project());

    assert!(view.visible);
    assert_eq!(view.title, "android");
    assert_eq!(view.label.as_deref(), Some("core-isol"));
    assert_eq!(view.branch.as_deref(), Some("codex/android-core-isol"));
}

#[test]
fn selected_project_hides_without_secondary() {
    let view = selected_project_view(3, "workspace", ProjectDetails::default());

    assert!(!view.visible);
    assert_eq!(view.label, None);
}

fn split_project() -> ProjectDetails {
    ProjectDetails {
        has_project: true,
        display_main: Some("android".to_owned()),
        display_secondary: Some("core-isol".to_owned()),
        icon: Some("developer_board".to_owned()),
        branch: Some("codex/android-core-isol".to_owned()),
    }
}
