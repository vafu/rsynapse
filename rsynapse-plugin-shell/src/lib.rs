use std::process::{Command, Stdio};

use rsynapse_plugin::{Plugin, ResultItem};

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

    fn query(&self, query: &str) -> Vec<ResultItem> {
        let command = query;
        if command.trim().is_empty() {
            return Vec::new();
        }

        if !is_valid_shell_syntax(query) {
            return Vec::new();
        }
        let full_command = format!("sh -c '{}'", command);

        vec![ResultItem {
            id: format!("shell-exec-{}", command),
            title: command.to_string(),
            description: Some("Execute as shell command".to_string()),
            icon: Some("utilities-terminal".to_string()),
            command: Some(full_command),
            score: std::f64::MIN,
        }]
    }
}

/// The plugin's FFI-safe entry point.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _rsynapse_init() -> *mut dyn Plugin {
    Box::into_raw(Box::new(ShellPlugin))
}
