use std::process::{Command, Stdio};

use rsynapse_plugin::{Plugin, PluginGroup, PluginResponse, QueryContext, ResultItem};

struct ShellPlugin;

fn is_valid_shell_syntax(query: &str) -> bool {
    if query.trim().is_empty() {
        return false;
    }

    let status = Command::new("sh")
        .arg("-n") // The "no-exec" flag for syntax checking.
        .arg("-c")
        .arg(query)
        .stdout(Stdio::null()) // Suppress any successful output.
        .stderr(Stdio::null()) // Suppress any syntax error messages.
        .status();

    match status {
        Ok(exit_status) => exit_status.success(),
        Err(_) => false,
    }
}

impl Plugin for ShellPlugin {
    fn name(&self) -> &'static str {
        "Shell Executor"
    }

    fn groups(&self) -> Vec<PluginGroup> {
        vec![PluginGroup {
            id: "shell".to_owned(),
            title: "Shell".to_owned(),
            description: "Run a shell command".to_owned(),
            aliases: vec!["sh".to_owned(), "run".to_owned(), "command".to_owned()],
            plugin: String::new(),
        }]
    }

    fn default_execute(&self) -> Option<&'static str> {
        None
    }

    fn execute(&self, item: &ResultItem) -> Option<Result<(), String>> {
        let data = if item.data.is_empty() {
            item.title.as_str()
        } else {
            item.data.as_str()
        };
        if data.trim().is_empty() {
            return Some(Err("empty shell command".to_owned()));
        }

        match Command::new("sh").arg("-c").arg(data).output() {
            Ok(output) if output.status.success() => Some(Ok(())),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stderr = stderr.trim();
                Some(Err(if stderr.is_empty() {
                    format!("shell command exited with {}", output.status)
                } else {
                    stderr.to_owned()
                }))
            }
            Err(error) => Some(Err(format!("failed to run shell command: {error}"))),
        }
    }

    fn query(&self, context: &QueryContext<'_>) -> PluginResponse {
        let command = context.query;
        if command.trim().is_empty() {
            return PluginResponse::default();
        }

        if !is_valid_shell_syntax(command) {
            return PluginResponse::default();
        }

        PluginResponse {
            view: None,
            results: vec![ResultItem {
                id: format!("shell-exec-{}", command),
                title: command.to_string(),
                description: "Execute as shell command".to_string(),
                icon: "utilities-terminal".to_string(),
                data: command.to_string(),
                score: std::f64::MIN,
                current: false,
            }],
        }
    }
}

/// The plugin's FFI-safe entry point.
pub fn plugin() -> Box<dyn Plugin> {
    Box::new(ShellPlugin)
}
