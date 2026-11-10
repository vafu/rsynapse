use super::*;

#[test]
fn activity_group_recovers_from_empty_and_keeps_independent_children_visible() {
    if std::env::var_os("GITHUB_GROUP_TEST_WORKER").is_none() {
        let name = concat!(
            module_path!(),
            "::activity_group_recovers_from_empty_and_keeps_independent_children_visible"
        );
        let name = name.split_once("::").unwrap().1;
        let binary = std::env::args_os().next().unwrap();
        let mut command =
            if let Ok(runner) = std::env::var("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER") {
                let args = shell_words::split(&runner).unwrap();
                let mut command = std::process::Command::new(&args[0]);
                command.args(&args[1..]).arg(binary);
                command
            } else {
                std::process::Command::new(binary)
            };
        assert!(
            command
                .args(["--exact", name, "--nocapture"])
                .env("GITHUB_GROUP_TEST_WORKER", "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    gtk::init().unwrap();
    let parent = gtk::Box::default();
    parent.set_visible(false);
    let group = ButtonGroup::default();
    parent.append(&group);
    let content = gtk::Box::default();
    let items = [
        gtk::Box::default(),
        gtk::Box::default(),
        gtk::Box::default(),
    ];
    for item in &items {
        item.set_visible(false);
        content.append(item);
    }
    group.set_content(&content);
    show_when_any_visible(&group, &items);
    assert!(!group.get_visible());

    // Re-review alone must reveal a previously empty activity group, even if
    // its enclosing surface has not mapped yet. Waiting changes is independent.
    items[1].set_visible(true);
    assert!(group.get_visible());
    items[2].set_visible(true);
    items[1].set_visible(false);
    assert!(group.get_visible());
    items[2].set_visible(false);
    assert!(!group.get_visible());
    items[0].set_visible(true);
    assert!(group.get_visible());

    let weak = group.downgrade();
    parent.remove(&group);
    drop(group);
    assert!(
        weak.upgrade().is_none(),
        "visibility handlers must not retain the group"
    );
}
