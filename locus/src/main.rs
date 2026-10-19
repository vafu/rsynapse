mod service;
mod store;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if matches!(args.as_slice(), [flag] if flag == "--help" || flag == "-h") {
        println!(
            "rsynapse-locusd — org.rsynapse.Locus session D-Bus relation daemon\n\
                  Usage: rsynapse-locusd\n\
                  Store: LOCUS_RELATIONS_PATH or $XDG_STATE_HOME/rsynapse/locus/relations.json\n\
                  Use locus --help for CLI commands."
        );
        return Ok(());
    }
    anyhow::ensure!(
        args.is_empty(),
        "rsynapse-locusd takes no arguments; use rsynapse-locusd --help"
    );
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rsynapse_locusd=info".into()),
        )
        .init();

    service::run().await
}
