use std::{fmt, io, path::PathBuf};

#[derive(Debug)]
pub(crate) enum StylesheetError {
    Read {
        path: PathBuf,
        source: io::Error,
    },
    UnsupportedExtension {
        path: PathBuf,
    },
    Watch {
        path: PathBuf,
        source: notify::Error,
    },
    SpawnSass {
        source: io::Error,
    },
    CompileScss {
        path: PathBuf,
        message: String,
    },
}

impl fmt::Display for StylesheetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(formatter, "failed to read {}: {source}", path.display())
            }
            Self::UnsupportedExtension { path } => {
                write!(
                    formatter,
                    "unsupported stylesheet extension for {}; expected css, scss, or sass",
                    path.display()
                )
            }
            Self::Watch { path, source } => {
                write!(formatter, "failed to watch {}: {source}", path.display())
            }
            Self::SpawnSass { source } => {
                write!(formatter, "failed to run sass compiler: {source}")
            }
            Self::CompileScss { path, message } => {
                write!(formatter, "failed to compile {}: {message}", path.display())
            }
        }
    }
}

impl std::error::Error for StylesheetError {}
