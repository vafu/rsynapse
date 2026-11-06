use std::fs;
use std::process::{Command, Stdio};
use std::sync::RwLock;

use regex::Regex;
use rsynapse_plugin::{Plugin, PluginGroup, PluginResponse, QueryContext, ResultItem};
use serde::Deserialize;

#[derive(Deserialize, Default)]
struct Config {
    #[serde(default)]
    plugins: std::collections::HashMap<String, PluginSection>,
}

#[derive(Deserialize, Default)]
struct PluginSection {
    #[serde(default)]
    commands: Vec<CommandEntry>,
}

#[derive(Deserialize)]
struct CommandEntry {
    pattern: String,
    command: String,
    #[serde(default)]
    modifiers: Modifiers,
}

#[derive(Deserialize, Default, Clone)]
struct Modifiers {
    id: Option<String>,
    title: Option<String>,
    description: Option<String>,
    icon: Option<String>,
    data: Option<String>,
}

struct CompiledCommand {
    pattern: Regex,
    command: String,
    modifiers: Modifiers,
}

#[derive(Deserialize)]
struct JsonResultItem {
    id: Option<String>,
    title: String,
    description: Option<String>,
    icon: Option<String>,
    data: Option<String>,
}

pub struct CommandsPlugin {
    commands: RwLock<Vec<CompiledCommand>>,
}

/// Builds a shell invocation whose user captures are positional arguments.
///
/// The configured template is trusted, but regex captures come from the query
/// and must never be spliced into the script text. `$1`, `$2`, ... in the
/// template resolve to the corresponding capture without being re-parsed by
/// the shell.
fn shell_command(command_template: &str, captures: &regex::Captures<'_>) -> Command {
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(command_template)
        .arg("rsynapse-command");
    for index in 1..captures.len() {
        command.arg(captures.get(index).map_or("", |capture| capture.as_str()));
    }
    command
}

impl CommandsPlugin {
    fn load() -> Self {
        let commands = Self::compile_commands();
        Self {
            commands: RwLock::new(commands),
        }
    }

    fn compile_commands() -> Vec<CompiledCommand> {
        let config = Self::load_config().unwrap_or_default();
        let entries = config
            .plugins
            .get("Commands")
            .map(|s| &s.commands[..])
            .unwrap_or_default();

        entries
            .iter()
            .filter_map(|entry| {
                Regex::new(&entry.pattern)
                    .ok()
                    .map(|pattern| CompiledCommand {
                        pattern,
                        command: entry.command.clone(),
                        modifiers: entry.modifiers.clone(),
                    })
            })
            .collect()
    }

    fn load_config() -> Option<Config> {
        let config_path = dirs::config_dir()?.join("rsynapse/config.toml");
        let content = fs::read_to_string(&config_path)
            .map_err(|e| {
                eprintln!(
                    "[Commands Plugin] Could not read config at {:?}: {}",
                    config_path, e
                )
            })
            .ok()?;
        toml::from_str(&content)
            .map_err(|e| eprintln!("[Commands Plugin] Failed to parse config: {}", e))
            .ok()
    }

    fn execute_command(
        command_template: &str,
        captures: &regex::Captures,
        modifiers: &Modifiers,
    ) -> Vec<ResultItem> {
        let output = match shell_command(command_template, captures)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
        {
            Ok(output) => output,
            Err(e) => {
                eprintln!("[Commands Plugin] Failed to run command: {}", e);
                return Vec::new();
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("[Commands Plugin] Command failed: {}", stderr);
            return Vec::new();
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Self::parse_output(&stdout, modifiers)
    }

    fn parse_output(output: &str, modifiers: &Modifiers) -> Vec<ResultItem> {
        // Try JSON array first, then fall back to jsonl (one object per line).
        let trimmed = output.trim();
        if trimmed.starts_with('[') {
            if let Ok(items) = serde_json::from_str::<Vec<JsonResultItem>>(trimmed) {
                return items
                    .into_iter()
                    .enumerate()
                    .map(|(i, item)| item.into_result(i, modifiers))
                    .collect();
            }
        }

        // jsonl: one JSON object per line
        trimmed
            .lines()
            .filter(|line| !line.trim().is_empty())
            .enumerate()
            .filter_map(|(i, line)| {
                serde_json::from_str::<JsonResultItem>(line)
                    .map(|item| item.into_result(i, modifiers))
                    .map_err(|e| eprintln!("[Commands Plugin] Failed to parse line: {}", e))
                    .ok()
            })
            .collect()
    }
}

impl JsonResultItem {
    fn into_result(self, index: usize, modifiers: &Modifiers) -> ResultItem {
        let id = self.id.unwrap_or_else(|| format!("commands-{}", index));
        let title = self.title;
        let description = self.description.unwrap_or_default();
        let icon = self.icon.unwrap_or_default();
        let data = self.data.unwrap_or_default();

        // Apply modifiers — each modifier is a template that can reference
        // the original field values via {id}, {title}, {description}, {icon}, {data}.
        let apply = |template: &str| -> String {
            template
                .replace("{id}", &id)
                .replace("{title}", &title)
                .replace("{description}", &description)
                .replace("{icon}", &icon)
                .replace("{data}", &data)
        };

        let final_id = modifiers
            .id
            .as_deref()
            .map(&apply)
            .unwrap_or_else(|| id.clone());
        let final_title = modifiers
            .title
            .as_deref()
            .map(&apply)
            .unwrap_or_else(|| title.clone());
        let final_desc = modifiers
            .description
            .as_deref()
            .map(&apply)
            .unwrap_or_else(|| description.clone());
        let final_icon = modifiers
            .icon
            .as_deref()
            .map(&apply)
            .unwrap_or_else(|| icon.clone());
        let final_data = modifiers
            .data
            .as_deref()
            .map(&apply)
            .unwrap_or_else(|| data.clone());

        ResultItem {
            id: final_id,
            title: final_title,
            description: final_desc,
            icon: final_icon,
            data: final_data,
            score: -(index as f64),
            current: false,
            keep_open: false,
        }
    }
}

impl Plugin for CommandsPlugin {
    fn name(&self) -> &'static str {
        "Commands"
    }

    fn groups(&self) -> Vec<PluginGroup> {
        vec![PluginGroup {
            id: "commands".to_owned(),
            title: "Commands".to_owned(),
            description: "Run configured command queries".to_owned(),
            aliases: vec!["cmd".to_owned(), "command".to_owned()],
            plugin: String::new(),
        }]
    }

    fn reload(&self) {
        eprintln!("[Commands Plugin] Reloading configuration...");
        let new_commands = Self::compile_commands();
        *self.commands.write().unwrap() = new_commands;
    }

    fn query(&self, context: &QueryContext<'_>) -> PluginResponse {
        let commands = self.commands.read().unwrap();
        let mut results = Vec::new();

        for cmd in commands.iter() {
            if let Some(captures) = cmd.pattern.captures(context.query) {
                results.extend(Self::execute_command(
                    &cmd.command,
                    &captures,
                    &cmd.modifiers,
                ));
            }
        }

        PluginResponse {
            view: None,
            results,
        }
    }
}

pub fn plugin() -> Box<dyn Plugin> {
    Box::new(CommandsPlugin::load())
}
