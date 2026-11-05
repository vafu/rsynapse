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
            description: Some("Run a shell command".to_owned()),
            aliases: vec!["sh".to_owned(), "run".to_owned(), "command".to_owned()],
        }]
    }

    fn default_execute(&self) -> Option<&'static str> {
        Some("{data}")
    }

    fn query(&self, context: &QueryContext<'_>) -> PluginResponse {
        let command = context.query;
        if command.trim().is_empty() {
            return PluginResponse::default();
        }

        if !is_valid_shell_syntax(command) {
            return PluginResponse::default();
        }
        let full_command = format!("sh -c '{}'", command);

        PluginResponse {
            view: None,
            results: vec![ResultItem {
                id: format!("shell-exec-{}", command),
                title: command.to_string(),
                description: Some("Execute as shell command".to_string()),
                icon: Some("utilities-terminal".to_string()),
                data: Some(full_command),
                score: std::f64::MIN,
                current: false,
            }],
        }
    }
}

/// The plugin's FFI-safe entry point.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _rsynapse_init() -> *mut dyn Plugin {
    Box::into_raw(Box::new(ShellPlugin))
}
