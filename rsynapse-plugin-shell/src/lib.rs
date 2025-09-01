use rsynapse_plugin::{Plugin, ResultItem};

const PREFIX: &str = "> ";

struct ShellPlugin;

impl Plugin for ShellPlugin {
    fn name(&self) -> &'static str {
        "Shell Executor"
    }

    fn query(&self, query: &str) -> Vec<ResultItem> {
        // Check if the query starts with our designated prefix.
        let command = query;
        // Don't return a result for an empty or whitespace-only command.
        if command.trim().is_empty() {
            return Vec::new();
        }

        // If it matches, return exactly one result item.
        // Wrapping the command in `sh -c '...'` allows the shell to correctly
        // interpret pipes, redirects, and other complex syntax.
        let full_command = format!("sh -c '{}'", command);

        vec![ResultItem {
            id: format!("shell-exec-{}", command),
            title: command.to_string(),
            description: Some("Execute as shell command".to_string()),
            icon: Some("utilities-terminal".to_string()),
            command: Some(full_command),
        }]
    }
}

/// The plugin's FFI-safe entry point.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _rsynapse_init() -> *mut dyn Plugin {
    Box::into_raw(Box::new(ShellPlugin))
}
