use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use super::{brightness_percent, find_backlight_device, read_brightness_view};

#[test]
fn brightness_percent_rounds_and_clamps() {
    assert_eq!(brightness_percent(0, 400), 0);
    assert_eq!(brightness_percent(1, 400), 0);
    assert_eq!(brightness_percent(201, 400), 50);
    assert_eq!(brightness_percent(400, 400), 100);
    assert_eq!(brightness_percent(800, 400), 100);
    assert_eq!(brightness_percent(12, 0), 0);
}

#[test]
fn finds_first_backlight_device_and_reads_percent() {
    let root = temp_root();
    write_backlight_device(&root, "z_backlight", 360, 400);
    write_backlight_device(&root, "a_backlight", 180, 400);

    let device = find_backlight_device(&root).unwrap();
    let view = read_brightness_view(&device).unwrap();

    assert_eq!(device.name, "a_backlight");
    assert!(view.visible);
    assert_eq!(view.percent, 45);

    let _ = fs::remove_dir_all(root);
}

fn write_backlight_device(root: &PathBuf, name: &str, brightness: u64, max: u64) {
    let dir = root.join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("brightness"), brightness.to_string()).unwrap();
    fs::write(dir.join("actual_brightness"), brightness.to_string()).unwrap();
    fs::write(dir.join("max_brightness"), max.to_string()).unwrap();
}

fn temp_root() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "rsynapse-brightness-test-{}-{suffix}",
        std::process::id()
    ))
}
