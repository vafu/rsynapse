mod desktop_icon;
pub use app_kit::{init_tracing, launch, request};
pub mod session;
mod theme;
pub mod widgets;

pub fn rsynapse_app(app_id: &str) -> shell_core::ShellApp {
    app_kit::rsynapse_app(
        app_id,
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/stylesheets/rsynapse-shell.scss"
        ),
    )
}
