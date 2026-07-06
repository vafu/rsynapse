pub mod paths;

pub use paths::{
    BUS_NAME, OUTPUT_INTERFACE, ROOT_INTERFACE, ROOT_PATH, WINDOW_INTERFACE, WORKSPACE_INTERFACE,
    output_path, window_path, workspace_path,
};

pub mod keys {
    pub const OUTPUT_NAME: &str = "org.rsynapse.niri.output.name";
    pub const WORKSPACE_ID: &str = "org.rsynapse.niri.workspace.id";
    pub const WORKSPACE_NAME: &str = "org.rsynapse.niri.workspace.name";
    pub const WINDOW_ID: &str = "org.rsynapse.niri.window.id";
}
