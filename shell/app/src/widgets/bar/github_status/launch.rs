use std::{ffi::OsStr, process::Stdio};

pub(super) async fn open(url: &str) -> Result<(), String> {
    open_with(OsStr::new("xdg-open"), url).await
}

async fn open_with(program: &OsStr, url: &str) -> Result<(), String> {
    // Do not ask GTK to export a layer-shell/popover parent as a regular
    // Wayland toplevel for URI activation. The external opener owns that path.
    let output = tokio::process::Command::new(program)
        .arg(url)
        .env_remove("XDG_ACTIVATION_TOKEN")
        .env_remove("DESKTOP_STARTUP_ID")
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|error| format!("could not launch URL opener: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "URL opener exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn url_is_one_literal_argument_and_opener_failure_is_reported() {
        let directory = tempfile::tempdir().unwrap();
        let program = directory.path().join("opener");
        std::fs::write(
            &program,
            "#!/bin/sh\nprintf '%s\\n' \"$#\" \"$1\" > \"$(dirname \"$0\")/received\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let url = "https://example.test/pull/7?x='quoted value'&y=$(no-shell);z=1";
        runtime
            .block_on(open_with(program.as_os_str(), url))
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(directory.path().join("received")).unwrap(),
            format!("1\n{url}\n")
        );
        std::fs::write(&program, "#!/bin/sh\nexit 7\n").unwrap();
        assert!(
            runtime
                .block_on(open_with(program.as_os_str(), url))
                .unwrap_err()
                .contains('7')
        );
    }
}
