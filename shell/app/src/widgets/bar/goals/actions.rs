use shell_core::{
    gtk::{self, prelude::*},
    source::proj::GoalInfo,
};

#[derive(Clone)]
pub(super) enum Action {
    Status { key: String, status: String },
    Track { project: String, key: String },
}
pub(super) fn run(
    action: Action,
    overlay: &adw::ToastOverlay,
    complete: impl FnOnce(bool) + 'static,
) {
    let (sender, receiver) = async_channel::bounded(1);
    relm4::spawn(async move {
        let result = async {
            let conn = zbus::Connection::session().await?;
            let proxy = zbus::Proxy::new(
                &conn,
                "org.rsynapse.Proj",
                "/org/rsynapse/Proj",
                "org.rsynapse.Proj.Manager1",
            )
            .await?;
            match action {
                Action::Status { key, status } => {
                    let _: GoalInfo = proxy.call("SetGoalStatus", &(key, status)).await?;
                }
                Action::Track { project, key } => {
                    let _: () = proxy.call("SetActiveGoal", &(project, key)).await?;
                }
            }
            Ok::<_, zbus::Error>(())
        }
        .await;
        let _ = sender.send(result.map_err(|e| e.to_string())).await;
    });
    let overlay = overlay.downgrade();
    gtk::glib::MainContext::default().spawn_local(async move {
        let Ok(result) = receiver.recv().await else {
            return;
        };
        complete(result.is_ok());
        if let Err(error) = result {
            if let Some(overlay) = overlay.upgrade() {
                let toast = adw::Toast::new(&error);
                toast.set_use_markup(false);
                overlay.add_toast(toast);
            }
        }
    });
}

fn executable(name: &str) -> bool {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .any(|p| p.join(name).is_file())
}
fn note_command(
    note_path: &str,
    editor: &str,
    terminal: &[String],
) -> Result<tokio::process::Command, String> {
    if note_path.is_empty() {
        return Err("This goal has no Markdown note path".into());
    }
    let editor = shell_words::split(editor).map_err(|e| e.to_string())?;
    if editor.is_empty() {
        return Err("EDITOR is empty".into());
    }
    let Some(program) = terminal.first() else {
        return Err("TERMINAL is empty".into());
    };
    let mut command = tokio::process::Command::new(program);
    command.args(&terminal[1..]).args(editor).arg(note_path);
    Ok(command)
}

pub(super) fn open_note(note_path: String, overlay: &adw::ToastOverlay) {
    let (sender, receiver) = async_channel::bounded(1);
    relm4::spawn(async move {
        let result = async {
            let terminal = if executable("xdg-terminal-exec") {
                vec!["xdg-terminal-exec".to_owned(), "--".to_owned()]
            } else {
                let value = std::env::var("TERMINAL").unwrap_or_else(|_| {
                    ["ghostty", "foot", "alacritty", "xterm"]
                        .into_iter()
                        .find(|name| executable(name))
                        .unwrap_or("xterm")
                        .to_owned()
                });
                let mut parts = shell_words::split(&value).map_err(|e| e.to_string())?;
                if parts.is_empty() {
                    return Err("TERMINAL is empty".to_owned());
                }
                parts.push("-e".into());
                parts
            };
            let editor = std::env::var("EDITOR")
                .or_else(|_| std::env::var("VISUAL"))
                .unwrap_or_else(|_| "vi".into());
            // NotePath already comes from projd's reactive goal snapshot. Avoid
            // resolving `proj` through PATH, which may contain an older helper.
            let status = note_command(&note_path, &editor, &terminal)?
                .status()
                .await
                .map_err(|e| format!("Could not open goal note: {e}"))?;
            if !status.success() {
                return Err(format!("Goal note terminal exited with {status}"));
            }
            Ok(())
        }
        .await;
        let _ = sender.send(result).await;
    });
    let overlay = overlay.downgrade();
    gtk::glib::MainContext::default().spawn_local(async move {
        if let Ok(Err(error)) = receiver.recv().await {
            if let Some(overlay) = overlay.upgrade() {
                let toast = adw::Toast::new(&error);
                toast.set_use_markup(false);
                overlay.add_toast(toast);
            }
        }
    });
}

#[cfg(test)]
mod test;
