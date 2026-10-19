#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "workspace_dbus=info".into()),
        )
        .init();
    let mut config = workspace_dbus::Config::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--backend" => {
                config.backend = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--backend needs a value"))?
                    .parse()?;
            }
            "--socket" => {
                config.socket = Some(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("--socket needs a path"))?
                        .into(),
                )
            }
            "--help" | "-h" => {
                println!(
                    "rsynapse-workspace-dbus [--backend auto|niri|umbriel] [--socket PATH]\nDefault: auto (XDG_CURRENT_DESKTOP, then unambiguous compositor socket).\nNiri: NIRI_SOCKET. Umbriel: UMBRIEL_SOCKET or $XDG_RUNTIME_DIR/umbriel-$WAYLAND_DISPLAY.sock."
                );
                return Ok(());
            }
            _ => anyhow::bail!("unknown argument: {arg}"),
        }
    }
    tokio::select! {
        result = workspace_dbus::run(config) => result,
        result = tokio::signal::ctrl_c() => { result?; Ok(()) }
    }
}
