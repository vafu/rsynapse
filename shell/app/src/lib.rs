mod desktop_icon;
mod hints;
pub mod request;
mod theme;
pub mod widgets;

use std::fs::File;
use std::path::PathBuf;
use std::time::Duration;

use shell_core::{ShellApp, css::CssPriority};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

const SHELL_STYLESHEET: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/stylesheets/rsynapse-shell.scss"
);
const SHELL_STYLESHEET_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/stylesheets");
const TRACE_CHROME_ENV: &str = "RSYNAPSE_TRACE_CHROME";
const TRACE_FILTER_ENV: &str = "RSYNAPSE_TRACE_FILTER";
const DEFAULT_TRACE_FILTER: &str =
    "rsynapse_shell=trace,shell_core=trace,gtk4_background_effect=trace";
const PPROF_FLAMEGRAPH_ENV: &str = "RSYNAPSE_PPROF_FLAMEGRAPH";
const PPROF_PROTO_ENV: &str = "RSYNAPSE_PPROF_PROTO";
const PPROF_SECONDS_ENV: &str = "RSYNAPSE_PPROF_SECONDS";
const PPROF_DELAY_SECONDS_ENV: &str = "RSYNAPSE_PPROF_DELAY_SECONDS";
const PPROF_FREQUENCY_ENV: &str = "RSYNAPSE_PPROF_FREQUENCY";
const DEFAULT_PPROF_SECONDS: u64 = 15;
const DEFAULT_PPROF_DELAY_SECONDS: u64 = 0;
const DEFAULT_PPROF_FREQUENCY: i32 = 1_000;

/// Initialize compact tracing from `RUST_LOG`.
pub fn init_tracing() {
    init_pprof();

    if let Some(path) = std::env::var_os(TRACE_CHROME_ENV).map(PathBuf::from) {
        init_chrome_tracing(path);
        return;
    }

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().compact())
        .with(EnvFilter::from_default_env())
        .init();
}

fn init_chrome_tracing(path: PathBuf) {
    let filter = trace_filter();
    let (chrome_layer, guard) = tracing_chrome::ChromeLayerBuilder::new()
        .file(path.clone())
        .include_args(true)
        .build();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(1));
            guard.flush();
        }
    });

    tracing_subscriber::registry()
        .with(filter)
        .with(chrome_layer)
        .init();

    eprintln!(
        "[rsynapse-shell] writing Perfetto-compatible trace to {}",
        path.display()
    );
}

fn init_pprof() {
    let flamegraph_path = std::env::var_os(PPROF_FLAMEGRAPH_ENV).map(PathBuf::from);
    let proto_path = std::env::var_os(PPROF_PROTO_ENV).map(PathBuf::from);
    if flamegraph_path.is_none() && proto_path.is_none() {
        return;
    }

    let seconds = env_u64(PPROF_SECONDS_ENV, DEFAULT_PPROF_SECONDS);
    let delay_seconds = env_u64(PPROF_DELAY_SECONDS_ENV, DEFAULT_PPROF_DELAY_SECONDS);
    let frequency = env_i32(PPROF_FREQUENCY_ENV, DEFAULT_PPROF_FREQUENCY);

    eprintln!(
        "[rsynapse-shell] scheduled pprof profile after {delay_seconds}s for {seconds}s at {frequency}Hz{}{}",
        output_description(" flamegraph", flamegraph_path.as_ref()),
        output_description(" protobuf", proto_path.as_ref())
    );

    std::thread::spawn(move || {
        if delay_seconds > 0 {
            std::thread::sleep(Duration::from_secs(delay_seconds));
        }

        let guard = match pprof::ProfilerGuardBuilder::default()
            .frequency(frequency)
            .blocklist(&["libc", "libgcc", "pthread", "vdso"])
            .build()
        {
            Ok(guard) => guard,
            Err(error) => {
                eprintln!("[rsynapse-shell] failed to start pprof profiler: {error}");
                return;
            }
        };

        eprintln!("[rsynapse-shell] recording pprof profile now");
        std::thread::sleep(Duration::from_secs(seconds));

        let report = match guard.report().build() {
            Ok(report) => report,
            Err(error) => {
                eprintln!("[rsynapse-shell] failed to build pprof report: {error}");
                return;
            }
        };

        if let Some(path) = flamegraph_path {
            write_pprof_flamegraph(&report, &path);
        }
        if let Some(path) = proto_path {
            write_pprof_proto(&report, &path);
        }
    });
}

fn write_pprof_flamegraph(report: &pprof::Report, path: &PathBuf) {
    let file = match File::create(path) {
        Ok(file) => file,
        Err(error) => {
            eprintln!(
                "[rsynapse-shell] failed to create pprof flamegraph {}: {error}",
                path.display()
            );
            return;
        }
    };

    if let Err(error) = report.flamegraph(file) {
        eprintln!(
            "[rsynapse-shell] failed to write pprof flamegraph {}: {error}",
            path.display()
        );
        return;
    }

    eprintln!(
        "[rsynapse-shell] wrote pprof flamegraph to {}",
        path.display()
    );
}

fn write_pprof_proto(report: &pprof::Report, path: &PathBuf) {
    let profile = match report.pprof() {
        Ok(profile) => profile,
        Err(error) => {
            eprintln!(
                "[rsynapse-shell] failed to build pprof protobuf {}: {error}",
                path.display()
            );
            return;
        }
    };

    let mut bytes = Vec::new();
    if let Err(error) = pprof::protos::Message::encode(&profile, &mut bytes) {
        eprintln!(
            "[rsynapse-shell] failed to encode pprof protobuf {}: {error}",
            path.display()
        );
        return;
    }

    if let Err(error) = std::fs::write(path, bytes) {
        eprintln!(
            "[rsynapse-shell] failed to write pprof protobuf {}: {error}",
            path.display()
        );
        return;
    }

    eprintln!(
        "[rsynapse-shell] wrote pprof protobuf to {}",
        path.display()
    );
}

fn output_description(kind: &str, path: Option<&PathBuf>) -> String {
    path.map(|path| format!("{kind}={}", path.display()))
        .unwrap_or_default()
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_i32(name: &str, default: i32) -> i32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn trace_filter() -> EnvFilter {
    if let Ok(filter) = std::env::var(TRACE_FILTER_ENV) {
        EnvFilter::new(filter)
    } else if let Ok(filter) = std::env::var(EnvFilter::DEFAULT_ENV) {
        EnvFilter::new(filter)
    } else {
        EnvFilter::new(DEFAULT_TRACE_FILTER)
    }
}

/// Build a rsynapse shell application with shared styling, theme, and Relm setup.
pub fn rsynapse_app(app_id: &str) -> ShellApp {
    ShellApp::new(app_id)
        .with_relm_threads(4)
        .with_sass_load_path(SHELL_STYLESHEET_DIR)
        .with_scss_at_priority(rsynapse_stylesheet(), CssPriority::User)
        .watch_stylesheets(true)
        .on_startup(|_| {
            adw::init().expect("failed to initialize libadwaita");
            theme::prepare_theme();
        })
}

fn rsynapse_stylesheet() -> PathBuf {
    let local = config_home().join("rsynapse/shell.scss");
    if local.exists() {
        local
    } else {
        PathBuf::from(SHELL_STYLESHEET)
    }
}

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
}
