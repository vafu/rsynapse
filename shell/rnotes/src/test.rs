use super::needs_bootstrap;

#[test]
fn bootstrap_only_when_zk_metadata_is_missing() {
    let temp = std::env::temp_dir().join(format!(
        "rnotes-bootstrap-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    assert!(needs_bootstrap(temp.to_str().unwrap()));
    std::fs::create_dir_all(temp.join(".zk")).unwrap();
    assert!(!needs_bootstrap(temp.to_str().unwrap()));
    std::fs::remove_dir_all(temp).unwrap();
}
