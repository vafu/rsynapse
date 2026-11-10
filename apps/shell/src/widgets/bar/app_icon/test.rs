use super::{FALLBACK_APPLICATION_ICON, icon_name_with_lookup};

#[test]
fn icon_name_falls_back_for_blank_values() {
    assert_eq!(
        icon_name_with_lookup("", |_| true),
        FALLBACK_APPLICATION_ICON
    );
    assert_eq!(
        icon_name_with_lookup("   ", |_| true),
        FALLBACK_APPLICATION_ICON
    );
}

#[test]
fn icon_name_falls_back_for_file_paths() {
    assert_eq!(
        icon_name_with_lookup("/tmp/app.png", |_| true),
        FALLBACK_APPLICATION_ICON
    );
}

#[test]
fn icon_name_falls_back_for_missing_theme_icons() {
    assert_eq!(
        icon_name_with_lookup("missing-app-icon", |_| false),
        FALLBACK_APPLICATION_ICON
    );
}

#[test]
fn icon_name_keeps_theme_icons() {
    assert_eq!(
        icon_name_with_lookup("org.example.App", |_| true),
        "org.example.App"
    );
}
