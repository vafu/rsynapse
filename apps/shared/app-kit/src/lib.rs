//! Shared application runtime for Rsynapse UI processes.
pub mod launch;
pub mod request;
mod tracing;

pub use tracing::init_tracing;

use std::path::Path;

use shell_core::ShellApp;

/// Build the standard Rsynapse GTK application startup.
pub fn rsynapse_app(app_id: &str, stylesheet: impl AsRef<Path>) -> ShellApp {
    let stylesheet = stylesheet.as_ref().to_path_buf();
    let style_dir = stylesheet
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    ShellApp::new(app_id)
        .with_relm_threads(4)
        .with_sass_load_path(style_dir)
        .with_scss_at_priority(stylesheet, shell_core::css::CssPriority::User)
        .watch_stylesheets(true)
        .on_startup(|_| {
            adw::init().expect("failed to initialize libadwaita");
            shell_core::gtk::Settings::default()
                .map(|settings| settings.set_gtk_application_prefer_dark_theme(false));
        })
}
