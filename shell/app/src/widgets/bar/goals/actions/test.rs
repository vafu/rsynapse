use super::note_command;
use std::{fs, os::unix::fs::PermissionsExt};

#[test]
fn opens_the_note_in_the_editor_even_when_a_legacy_proj_shadows_path() {
    let temp = tempfile::tempdir().unwrap();
    let note = temp.path().join("goal note with spaces.md");
    fs::write(&note, "# Existing context\n").unwrap();
    let editor = temp.path().join("editor fixture");
    fs::write(
        &editor,
        "test -f \"$2\" || exit 2\nprintf '%s\\n' \"$@\" > \"$NOTE_ARGS_LOG\"\n",
    )
    .unwrap();
    let terminal = temp.path().join("terminal fixture");
    fs::write(
        &terminal,
        "case \"$1\" in --|-e) shift;; *) exit 3;; esac\nexec \"$@\"\n",
    )
    .unwrap();
    let legacy = temp.path().join("proj");
    fs::write(
        &legacy,
        "#!/bin/sh\nprintf 'legacy proj called' > \"$LEGACY_LOG\"\nexit 91\n",
    )
    .unwrap();
    fs::set_permissions(&legacy, fs::Permissions::from_mode(0o755)).unwrap();
    let log = temp.path().join("editor-args");
    let legacy_log = temp.path().join("legacy-called");
    let editor = format!(
        "/bin/sh {} '--marker=two words'",
        shell_words::quote(editor.to_str().unwrap())
    );
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            for flag in ["-e", "--"] {
                let terminal = vec![
                    "/bin/sh".into(),
                    terminal.to_str().unwrap().into(),
                    flag.into(),
                ];
                let status = note_command(note.to_str().unwrap(), &editor, &terminal)
                    .unwrap()
                    .env("PATH", temp.path())
                    .env("NOTE_ARGS_LOG", &log)
                    .env("LEGACY_LOG", &legacy_log)
                    .status()
                    .await
                    .unwrap();
                assert!(status.success());
                assert_eq!(
                    fs::read_to_string(&log).unwrap(),
                    format!("--marker=two words\n{}\n", note.display())
                );
                assert!(!legacy_log.exists());
                assert_eq!(fs::read_to_string(&note).unwrap(), "# Existing context\n");
            }
        });
}
