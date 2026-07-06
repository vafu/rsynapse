use std::path::PathBuf;

use notify::{
    Event, EventKind,
    event::{DataChange, MetadataKind, ModifyKind},
};

use super::{SassConfig, StylesheetSource, watcher::is_stylesheet_reload_event};

#[test]
fn css_source_watches_file_path() {
    let source = StylesheetSource::new("styles/main.css");

    assert_eq!(
        source.watch_roots(&SassConfig::default()).unwrap(),
        vec![PathBuf::from("styles/main.css")]
    );
}

#[test]
fn sass_source_watches_parent_directory_and_load_paths() {
    let mut sass_config = SassConfig::default();
    sass_config.add_load_path("shared/styles");
    sass_config.add_load_path("vendor/styles");
    let source = StylesheetSource::new("styles/main.scss");

    assert_eq!(
        source.watch_roots(&sass_config).unwrap(),
        vec![
            PathBuf::from("shared/styles"),
            PathBuf::from("styles"),
            PathBuf::from("vendor/styles"),
        ]
    );
}

#[test]
fn source_rejects_unknown_stylesheet_extensions() {
    let source = StylesheetSource::new("styles/main.txt");

    assert!(source.watch_roots(&SassConfig::default()).is_err());
}

#[test]
fn watcher_accepts_stylesheet_content_events() {
    let event = Event {
        kind: EventKind::Modify(ModifyKind::Data(DataChange::Content)),
        paths: vec![PathBuf::from("styles/main.scss")],
        attrs: Default::default(),
    };

    assert!(is_stylesheet_reload_event(&event));
}

#[test]
fn watcher_ignores_unrelated_paths() {
    let event = Event {
        kind: EventKind::Modify(ModifyKind::Data(DataChange::Content)),
        paths: vec![PathBuf::from("styles/notes.txt")],
        attrs: Default::default(),
    };

    assert!(!is_stylesheet_reload_event(&event));
}

#[test]
fn watcher_ignores_metadata_noise() {
    let event = Event {
        kind: EventKind::Modify(ModifyKind::Metadata(MetadataKind::Any)),
        paths: vec![PathBuf::from("styles/main.css")],
        attrs: Default::default(),
    };

    assert!(!is_stylesheet_reload_event(&event));
}
