use std::ffi::OsStr;

use rsynapse_shell::{
    init_tracing,
    launch::LaunchMode,
    request, rsynapse_app,
    widgets::{MainBar, MainBarInit},
};

fn main() {
    let mut args = std::env::args_os();
    let binary = args.next().unwrap_or_default();
    let command = args.next();
    if command.as_deref() == Some(OsStr::new("request")) {
        std::process::exit(request::run_cli(args));
    }

    let launch_mode = LaunchMode::from_arg(command.as_deref());

    init_tracing();

    launch_mode
        .apply(
            rsynapse_app("org.rsynapse.Shell"),
            binary.to_string_lossy().into_owned(),
        )
        .run_async::<MainBar>(MainBarInit::primary("Rsynapse Shell"));
}
