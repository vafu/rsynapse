use super::{SessionSnapshot, select_session_path};

#[test]
fn selects_active_wayland_session_for_uid() {
    let sessions = vec![
        session(1001, "/org/freedesktop/login1/session/_10", true, "tty"),
        session(1001, "/org/freedesktop/login1/session/_31", true, "wayland"),
    ];

    assert_eq!(
        select_session_path(&sessions, 1001),
        Some("/org/freedesktop/login1/session/_31")
    );
}

#[test]
fn ignores_other_users_sessions() {
    let sessions = vec![session(
        1002,
        "/org/freedesktop/login1/session/_31",
        true,
        "wayland",
    )];

    assert_eq!(select_session_path(&sessions, 1001), None);
}

#[test]
fn does_not_guess_when_no_active_wayland_session_exists() {
    let sessions = vec![
        session(
            1001,
            "/org/freedesktop/login1/session/_10",
            false,
            "wayland",
        ),
        session(1001, "/org/freedesktop/login1/session/_11", true, "tty"),
    ];

    assert_eq!(select_session_path(&sessions, 1001), None);
}

fn session(uid: u32, path: &str, active: bool, session_type: &str) -> SessionSnapshot {
    SessionSnapshot {
        uid,
        path: path.to_owned(),
        active,
        session_type: session_type.to_owned(),
    }
}
