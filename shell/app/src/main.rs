use std::ffi::OsStr;

use rsynapse_shell::{
    init_tracing, request, rsynapse_app,
    widgets::{MainBar, MainBarInit},
};

fn main() {
    let mut args = std::env::args_os();
    let _binary = args.next();
    if args.next().as_deref() == Some(OsStr::new("request")) {
        std::process::exit(request::run_cli(args));
    }

    init_tracing();

    rsynapse_app("org.rsynapse.Shell").run_async::<MainBar>(MainBarInit {
        title: "Rsynapse Shell",
    });
}
