//! Small XDG base-directory resolver shared by services.
//!
//! Each function prefers the corresponding `XDG_*` environment variable,
//! falls back to the documented location under `$HOME`, and returns a clear
//! error when neither is available. This is deliberately not a config
//! framework: callers only need a base directory.

use std::{
    ffi::OsString,
    fmt,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XdgError {
    /// The XDG variable and `$HOME` were both unset or empty.
    MissingHome(&'static str),
}

impl fmt::Display for XdgError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHome(variable) => write!(
                formatter,
                "{variable} is unset and HOME is unavailable; pass an explicit path"
            ),
        }
    }
}

impl std::error::Error for XdgError {}

/// `$XDG_STATE_HOME`, else `$HOME/.local/state`.
pub fn state_home() -> Result<PathBuf, XdgError> {
    resolve(
        std::env::var_os("XDG_STATE_HOME"),
        std::env::var_os("HOME"),
        ".local/state",
        "XDG_STATE_HOME",
    )
}

/// `$XDG_CONFIG_HOME`, else `$HOME/.config`.
pub fn config_home() -> Result<PathBuf, XdgError> {
    resolve(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
        ".config",
        "XDG_CONFIG_HOME",
    )
}

/// `$XDG_DATA_HOME`, else `$HOME/.local/share`.
pub fn data_home() -> Result<PathBuf, XdgError> {
    resolve(
        std::env::var_os("XDG_DATA_HOME"),
        std::env::var_os("HOME"),
        ".local/share",
        "XDG_DATA_HOME",
    )
}

/// `$XDG_CACHE_HOME`, else `$HOME/.cache`.
pub fn cache_home() -> Result<PathBuf, XdgError> {
    resolve(
        std::env::var_os("XDG_CACHE_HOME"),
        std::env::var_os("HOME"),
        ".cache",
        "XDG_CACHE_HOME",
    )
}

/// Turns a possibly relative path into an absolute one using the current
/// directory. Absolute paths are returned unchanged. Callers that must not
/// depend on the process CWD should pass an absolute path explicitly.
pub fn absolute(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        return path;
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(&path))
        .unwrap_or(path)
}

/// Parent directory of a path, treating a bare filename as the current
/// directory instead of an empty path segment.
pub fn parent_or_current(path: &Path) -> PathBuf {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    }
}

fn resolve(
    value: Option<OsString>,
    home: Option<OsString>,
    fallback: &str,
    variable: &'static str,
) -> Result<PathBuf, XdgError> {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(value));
    }
    let home = home
        .filter(|home| !home.is_empty())
        .ok_or(XdgError::MissingHome(variable))?;
    Ok(PathBuf::from(home).join(fallback))
}

#[cfg(test)]
mod tests {
    use super::{XdgError, absolute, parent_or_current, resolve};
    use std::{
        ffi::OsString,
        path::{Path, PathBuf},
    };

    #[test]
    fn explicit_xdg_value_wins() {
        assert_eq!(
            resolve(
                Some(OsString::from("/custom/state")),
                Some(OsString::from("/home/user")),
                ".local/state",
                "XDG_STATE_HOME"
            ),
            Ok(PathBuf::from("/custom/state"))
        );
    }

    #[test]
    fn empty_xdg_value_falls_back_to_home() {
        assert_eq!(
            resolve(
                Some(OsString::new()),
                Some(OsString::from("/home/user")),
                ".local/state",
                "XDG_STATE_HOME"
            ),
            Ok(PathBuf::from("/home/user/.local/state"))
        );
    }

    #[test]
    fn missing_home_is_an_error() {
        assert_eq!(
            resolve(None, None, ".local/state", "XDG_STATE_HOME"),
            Err(XdgError::MissingHome("XDG_STATE_HOME"))
        );
    }

    #[test]
    fn absolute_keeps_absolute_paths() {
        assert_eq!(
            absolute(PathBuf::from("/already/absolute")),
            PathBuf::from("/already/absolute")
        );
    }

    #[test]
    fn parent_of_bare_filename_is_not_empty() {
        assert!(
            !parent_or_current(Path::new("store.db"))
                .as_os_str()
                .is_empty()
        );
    }
}
