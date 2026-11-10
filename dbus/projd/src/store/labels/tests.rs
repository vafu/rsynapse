use crate::store::Store;
use proj_model::ProjectInfo;
use std::sync::Arc;

fn project() -> ProjectInfo {
    ProjectInfo {
        id: "p".into(),
        name: "Project".into(),
        cwd: "/project".into(),
        checkout_id: String::new(),
        icon: String::new(),
        icon_origin: String::new(),
    }
}

fn edit(store: &Store, labels: &[&str], add: bool) -> (Vec<String>, bool) {
    store
        .edit_project_labels("p", labels.iter().map(|s| s.to_string()).collect(), add)
        .unwrap()
        .unwrap()
}

#[test]
fn old_records_restart_noop_writes_and_deletion() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("store.db");
    let store = Store::open(&path).unwrap();
    store.put_project(&project(), None).unwrap();
    assert!(store.project_labels("p").unwrap().is_empty());
    let changes = || store.db.lock().unwrap().total_changes();
    let before = changes();
    assert_eq!(edit(&store, &[], true), (vec![], false));
    assert_eq!(edit(&store, &["absent"], false), (vec![], false));
    assert_eq!(changes(), before);
    assert_eq!(
        edit(&store, &["z", "a", "a"], true),
        (vec!["a".into(), "z".into()], true)
    );
    let before = changes();
    assert!(!edit(&store, &["a"], true).1);
    assert!(!edit(&store, &["absent"], false).1);
    assert_eq!(changes(), before);
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.project_labels("p").unwrap(), vec!["a", "z"]);
    assert_eq!(edit(&store, &["a"], false), (vec!["z".into()], true));
    // A failure deleting labels must roll back the project deletion too.
    store.db.lock().unwrap().execute_batch(
        "CREATE TEMP TRIGGER fail_labels BEFORE DELETE ON records WHEN OLD.kind='project-labels' BEGIN SELECT RAISE(ABORT,'fixture'); END;"
    ).unwrap();
    assert!(store.remove_project("p").is_err());
    assert_eq!(store.projects().unwrap(), vec![project()]);
    assert_eq!(store.project_labels("p").unwrap(), vec!["z"]);
    store
        .db
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_labels")
        .unwrap();
    store.remove_project("p").unwrap().unwrap();
    assert!(store.project_labels("p").unwrap().is_empty());
    assert!(
        store
            .edit_project_labels("p", vec!["late".into()], true)
            .unwrap()
            .is_none()
    );
    // Re-registering the same directory with a fresh identity inherits nothing.
    let fresh = ProjectInfo {
        id: "fresh".into(),
        ..project()
    };
    store.put_project(&fresh, None).unwrap();
    assert!(store.project_labels("fresh").unwrap().is_empty());
    drop(store);
    let store = Store::open(&path).unwrap();
    assert!(store.project_labels("p").unwrap().is_empty());
    assert_eq!(store.projects().unwrap(), vec![fresh]);
}

#[test]
fn concurrent_batches_metadata_and_delete_do_not_lose_or_resurrect_labels() {
    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&temp.path().join("store.db")).unwrap());
    store.put_project(&project(), None).unwrap();
    edit(&store, &["remove-me"], true);
    std::thread::scope(|scope| {
        for i in 0..24 {
            let store = store.clone();
            scope.spawn(move || {
                store
                    .edit_project_labels("p", vec![format!("label-{i:02}")], true)
                    .unwrap()
                    .unwrap();
            });
        }
        scope.spawn(|| {
            edit(&store, &["remove-me"], false);
        });
        scope.spawn(|| {
            let p = ProjectInfo {
                name: "Renamed".into(),
                icon: "M".into(),
                ..project()
            };
            store.put_project(&p, None).unwrap();
        });
    });
    assert_eq!(
        store.project_labels("p").unwrap(),
        (0..24).map(|i| format!("label-{i:02}")).collect::<Vec<_>>()
    );
    let p = store.projects().unwrap().pop().unwrap();
    assert_eq!((p.name.as_str(), p.icon.as_str()), ("Renamed", "M"));
    std::thread::scope(|scope| {
        scope.spawn(|| {
            store.remove_project("p").unwrap().unwrap();
        });
        for _ in 0..24 {
            scope.spawn(|| {
                store
                    .edit_project_labels("p", vec!["racing".into()], true)
                    .unwrap();
            });
        }
    });
    assert!(store.projects().unwrap().is_empty());
    assert!(store.project_labels("p").unwrap().is_empty());
}
