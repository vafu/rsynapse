use std::ffi::OsStr;

use shell_core::ShellApp;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LaunchMode {
    #[default]
    Normal,
    Inspector,
}

impl LaunchMode {
    pub fn from_arg(arg: Option<&OsStr>) -> Self {
        match arg {
            Some(arg) if arg == OsStr::new("inspect") => Self::Inspector,
            _ => Self::Normal,
        }
    }

    /// Configure application startup for this mode.
    pub fn apply(self, app: ShellApp, binary: String) -> ShellApp {
        match self {
            Self::Normal => app,
            Self::Inspector => app
                .with_args([binary])
                .on_startup(|_| gtk::Window::set_interactive_debugging(true)),
        }
    }
}

#[cfg(test)]
mod test;
