use std::ffi::OsStr;

use super::LaunchMode;

#[test]
fn parses_inspector_launch_mode() {
    assert_eq!(
        LaunchMode::from_arg(Some(OsStr::new("inspect"))),
        LaunchMode::Inspector
    );
}

#[test]
fn defaults_to_normal_launch_mode() {
    assert_eq!(LaunchMode::from_arg(None), LaunchMode::Normal);
    assert_eq!(
        LaunchMode::from_arg(Some(OsStr::new("request"))),
        LaunchMode::Normal
    );
}
