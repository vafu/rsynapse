use super::*;

fn task(instance: &str, key: &str, project_id: &str) -> GoalIdentityInfo {
    GoalIdentityInfo {
        instance_id: instance.to_string(),
        key: key.to_string(),
        goal: proj_model::GoalInfo {
            id: key.to_string(),
            project_id: project_id.to_string(),
            ..Default::default()
        },
        project_name: String::new(),
        created_at_ms: 0,
        observed_at_ms: 0,
        deleted_at_ms: 0,
        ancestor_ids: Vec::new(),
    }
}

fn project(id: &str, name: &str) -> ProjectInfo {
    ProjectInfo {
        id: id.to_string(),
        name: name.to_string(),
        cwd: "/tmp/example".to_string(),
        checkout_id: String::new(),
        icon: String::new(),
        icon_origin: String::new(),
    }
}

#[test]
fn task_selection_accepts_instance_key_or_id() {
    let rows = vec![
        task("instance-1", "ship", "p1"),
        task("instance-2", "ship", "p2"),
    ];
    let one = project("p1", "One");
    assert_eq!(
        select_task(rows.clone(), Some(&one), "instance-1")
            .unwrap()
            .instance_id,
        "instance-1"
    );
    assert_eq!(
        select_task(rows.clone(), Some(&one), "ship")
            .unwrap()
            .instance_id,
        "instance-1"
    );
    assert!(select_task(rows, None, "ship").is_err());
}

#[test]
fn task_selection_rejects_foreign_project() {
    let one = project("p1", "One");
    let rows = vec![task("instance-2", "ship", "p2")];
    assert!(select_task(rows, Some(&one), "instance-2").is_err());
}

#[test]
fn project_notebook_uses_task_notes_convention() {
    assert_eq!(
        task_notes::notebook_path("/tmp/example"),
        PathBuf::from("/tmp/example/.notes")
    );
}

#[test]
fn checkpoint_round_trips_and_clears() {
    let dir = std::env::temp_dir().join(format!(
        "rnotes-checkpoint-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let cwd = dir.to_str().unwrap();

    assert_eq!(read_checkpoint(cwd).unwrap(), None);
    let path = write_checkpoint(cwd, "ship").unwrap();
    assert_eq!(path, dir.join(".rnotes-task"));
    assert_eq!(read_checkpoint(cwd).unwrap().as_deref(), Some("ship"));
    clear_checkpoint(cwd).unwrap();
    assert_eq!(read_checkpoint(cwd).unwrap(), None);
    clear_checkpoint(cwd).unwrap();

    std::fs::remove_dir_all(dir).unwrap();
}
