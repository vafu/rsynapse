mod model;
mod niri;
mod row;
mod window;

use shell_core::ShellApp;

fn main() {
    ShellApp::new("org.rsynapse.WindowTilesExample")
        .with_relm_threads(2)
        .run_async::<window::WindowTiles>(());
}
