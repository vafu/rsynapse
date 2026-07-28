use std::{fmt, io, path::PathBuf};

#[derive(Debug)]
pub(crate) enum PolicyConfigError {
    Toml(toml::de::Error),
    Regex {
        rule_index: usize,
        field: &'static str,
        source: regex::Error,
    },
}

impl fmt::Display for PolicyConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toml(error) => write!(formatter, "invalid TOML: {error}"),
            Self::Regex {
                rule_index,
                field,
                source,
            } => write!(
                formatter,
                "invalid regex in rules[{rule_index}].{field}: {source}"
            ),
        }
    }
}

impl std::error::Error for PolicyConfigError {}

#[derive(Debug)]
pub(super) enum PolicyLoadError {
    NotFound,
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Config {
        path: PathBuf,
        source: PolicyConfigError,
    },
}

impl fmt::Display for PolicyLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(formatter, "notification center config not found"),
            Self::Io { path, source } => {
                write!(formatter, "failed to read {}: {source}", path.display())
            }
            Self::Config { path, source } => {
                write!(formatter, "failed to parse {}: {source}", path.display())
            }
        }
    }
}
