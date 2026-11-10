use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

use super::StylesheetError;

const SASS_EXECUTABLE_ENV: &str = "LOCUS_SHELL_SASS";
const DEFAULT_SASS_EXECUTABLE: &str = "sass";

#[derive(Debug, Clone, Default)]
pub(crate) struct SassConfig {
    load_paths: Vec<PathBuf>,
}

impl SassConfig {
    pub(crate) fn add_load_path(&mut self, path: impl Into<PathBuf>) {
        self.load_paths.push(path.into());
    }

    pub(crate) fn load_paths(&self) -> &[PathBuf] {
        &self.load_paths
    }

    fn executable(&self) -> PathBuf {
        env::var_os(SASS_EXECUTABLE_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_SASS_EXECUTABLE))
    }
}

pub(super) fn compile_scss(path: &Path, config: &SassConfig) -> Result<String, StylesheetError> {
    let mut command = Command::new(config.executable());
    command.arg("--no-source-map");

    for load_path in config.load_paths() {
        command.arg("--load-path").arg(load_path);
    }

    let output = command
        .arg(path)
        .output()
        .map_err(|source| StylesheetError::SpawnSass { source })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(StylesheetError::CompileScss {
            path: path.to_path_buf(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}
