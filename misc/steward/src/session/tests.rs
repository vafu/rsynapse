use super::{model::*, store::Store};

fn snapshot(name: &str) -> Session {
    Session {
        version: 1,
        name: name.into(),
        workspaces: vec![Workspace {
            id: id(),
            binding: id(),
            output: "missing-monitor".into(),
            index: 1,
            name: "code".into(),
            layout: workspace_dbus_api::Layout::Scrolling,
            windows: vec![Window {
                id: id(),
                app_id: "example".into(),
                floating: false,
                column: vec![0],
                row: vec![0],
                recipe: vec![Recipe {
                    provider: "configured".into(),
                    version: 1,
                    argv: vec!["example".into(), "{restore_id}".into()],
                    cwd: "/tmp".into(),
                    match_app_id: "example.{restore_id}".into(),
                }],
            }],
        }],
    }
}

#[test]
fn durable_store_roundtrips_and_preserves_named_and_previous_checkpoints() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sessions.json");
    let mut store = Store::open(path.clone()).unwrap();
    let first = snapshot("last");
    store.put(first.clone()).unwrap();
    let mut named = first.clone();
    named.name = "work".into();
    store.put(named.clone()).unwrap();
    let second = snapshot("last");
    store.put(second.clone()).unwrap();
    let reopened = Store::open(path).unwrap();
    assert_eq!(reopened.get("last").unwrap(), second);
    assert_eq!(reopened.get("work").unwrap(), named);
    let mut previous = first;
    previous.name = "previous".into();
    assert_eq!(reopened.get("previous").unwrap(), previous);
}

#[test]
fn invalid_schemas_or_duplicate_identities_cannot_replace_a_good_save() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sessions.json");
    let mut store = Store::open(path.clone()).unwrap();
    let first = snapshot("last");
    store.put(first.clone()).unwrap();
    let mut invalid = first.clone();
    invalid.version = 99;
    assert!(store.put(invalid).is_err());
    let mut invalid = first.clone();
    invalid.workspaces[0].windows[0].id = invalid.workspaces[0].id.clone();
    assert!(store.put(invalid).is_err());
    assert_eq!(Store::open(path).unwrap().get("last").unwrap(), first);
}

#[test]
fn failed_persistence_keeps_in_memory_state_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store");
    let mut store = Store::open(path.clone()).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(store.put(snapshot("last")).is_err());
    assert!(store.list().is_empty());
}

#[test]
fn live_links_are_scoped_to_source_owner_and_generation() {
    use super::{links, observe};
    let mut source = observe::Snapshot {
        owner: ":1.1".into(),
        generation: 2,
        outputs: Default::default(),
        workspaces: Default::default(),
        windows: Default::default(),
    };
    let record = locus::RelationRecord {
        subject: locus::RelationEndpoint::stable_key(locus::keys::WORKSPACE_ID, "opaque"),
        relation: links::LIVE_SLOT.into(),
        target: locus::RelationEndpoint::stable_key(links::SLOT, id()),
        metadata: links::metadata(Some(&source)),
        created_at_unix_ms: 0,
        updated_at_unix_ms: 0,
    };
    assert!(links::current(&record, &source));
    source.owner = ":1.2".into();
    assert!(!links::current(&record, &source));
    source.owner = ":1.1".into();
    source.generation += 1;
    assert!(!links::current(&record, &source));
    assert_ne!(links::SLOT, locus::keys::WORKSPACE_ID);
}

#[test]
fn terminal_recipe_has_explicit_launch_identity_and_preserves_cwd_arguments() {
    use super::launch;
    let directory = tempfile::Builder::new()
        .prefix("project with spaces ")
        .tempdir()
        .unwrap();
    let cwd = directory.path().to_str().unwrap();
    let recipe = launch::Rules::default()
        .recipe("com.mitchellh.ghostty", cwd)
        .unwrap();
    let (argv, app) = launch::expanded(&recipe, "unique").unwrap();
    assert_eq!(app, "com.mitchellh.ghostty.rsynapse.runique");
    assert!(argv.contains(&format!("--working-directory={cwd}")));
    assert!(argv.contains(&"--gtk-single-instance=false".into()));
    assert!(argv.contains(&format!("--class={app}")));
    let mut unsupported = recipe.clone();
    unsupported.version = 2;
    assert!(launch::expanded(&unsupported, "unique").is_err());
    unsupported.version = 1;
    unsupported.provider = "future-content-provider".into();
    assert!(launch::expanded(&unsupported, "unique").is_err());
}
