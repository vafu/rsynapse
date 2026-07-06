mod dbus;
mod ipc;
mod paths;
mod service;
mod state;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "niri_dbus=info".into()),
        )
        .init();

    service::run().await
}
