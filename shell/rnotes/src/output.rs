use anyhow::Result;
use clap::ValueEnum;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum Format {
    #[default]
    Text,
    Json,
    Jsonl,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScopeView {
    pub notebook: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PageInfo {
    pub limit: u64,
    pub truncated: bool,
}

pub struct Output {
    pub command: &'static str,
    pub data: Value,
    pub page: Option<PageInfo>,
    pub text: String,
}

#[derive(Serialize)]
struct Envelope<'a> {
    schema_version: u16,
    command: &'a str,
    scope: &'a ScopeView,
    data: &'a Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    page: &'a Option<PageInfo>,
}

pub fn emit(format: Format, quiet: bool, scope: &ScopeView, output: &Output) -> Result<()> {
    match format {
        Format::Text => {
            if !quiet && !output.text.is_empty() {
                println!("{}", output.text);
            }
        }
        Format::Json => println!(
            "{}",
            serde_json::to_string(&Envelope {
                schema_version: 1,
                command: output.command,
                scope,
                data: &output.data,
                page: &output.page,
            })?
        ),
        Format::Jsonl => emit_jsonl(scope, output)?,
    }
    Ok(())
}

fn emit_jsonl(scope: &ScopeView, output: &Output) -> Result<()> {
    if let Value::Array(records) = &output.data {
        for record in records {
            println!(
                "{}",
                serde_json::to_string(&Envelope {
                    schema_version: 1,
                    command: output.command,
                    scope,
                    data: record,
                    page: &None,
                })?
            );
        }
    } else {
        println!(
            "{}",
            serde_json::to_string(&Envelope {
                schema_version: 1,
                command: output.command,
                scope,
                data: &output.data,
                page: &output.page,
            })?
        );
    }
    Ok(())
}

#[derive(Debug)]
pub struct CliError {
    pub code: &'static str,
    pub message: String,
    pub details: BTreeMap<String, String>,
    pub exit: u8,
}

impl CliError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid-input", 2, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("not-found", 3, message)
    }

    pub fn rejected(message: impl Into<String>) -> Self {
        Self::new("mutation-rejected", 6, message)
    }

    pub fn new(code: &'static str, exit: u8, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: BTreeMap::new(),
            exit,
        }
    }

    pub fn detail(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}

pub fn classify(error: anyhow::Error) -> CliError {
    if let Some(error) = error.downcast_ref::<CliError>() {
        return CliError {
            code: error.code,
            message: error.message.clone(),
            details: error.details.clone(),
            exit: error.exit,
        };
    }
    let display = format!("{error:#}");
    let haystack = format!("{display}\n{error:?}").to_lowercase();
    let code = if haystack.contains("org.remarked.error.notfound") {
        ("not-found", 3)
    } else if haystack.contains("org.remarked.error.capabilityunavailable") {
        ("capability-unavailable", 4)
    } else if haystack.contains("org.remarked.error.revisionconflict") {
        ("revision-conflict", 5)
    } else if haystack.contains("org.remarked.error.mutationrejected") {
        ("mutation-rejected", 6)
    } else if haystack.contains("org.remarked.error.invalidinput") {
        ("invalid-input", 2)
    } else if haystack.contains("serviceunknown")
        || haystack.contains("namehasnoowner")
        || haystack.contains("not activatable")
        || haystack.contains("connect to remarked")
        || haystack.contains("connect to projd")
    {
        ("transport-unavailable", 7)
    } else {
        ("internal", 1)
    };
    CliError::new(code.0, code.1, display)
}

pub fn emit_error(format: Format, error: &CliError) {
    match format {
        Format::Text => eprintln!("{}: {}", error.code, error.message),
        Format::Json | Format::Jsonl => {
            #[derive(Serialize)]
            struct Body<'a> {
                code: &'static str,
                message: &'a str,
                details: &'a BTreeMap<String, String>,
            }
            #[derive(Serialize)]
            struct Envelope<'a> {
                schema_version: u16,
                error: Body<'a>,
            }
            let envelope = Envelope {
                schema_version: 1,
                error: Body {
                    code: error.code,
                    message: &error.message,
                    details: &error.details,
                },
            };
            eprintln!(
                "{}",
                serde_json::to_string(&envelope).unwrap_or_else(|_| {
                    "{\"schema_version\":1,\"error\":{\"code\":\"internal\",\"message\":\"failed to serialize error\",\"details\":{}}}".to_string()
                })
            );
        }
    }
}
