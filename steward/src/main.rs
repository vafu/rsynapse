mod agent_metrics;
mod associations;
mod focus;
mod git_status;
mod metrics;
mod namer;
mod productivity;
mod relations;
mod workdays;

use crate::relations::LocusClient;
use git_status::GitStatus;
use metrics::Metrics;
use namer::Namer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if productivity::cli::run().await? {
        return Ok(());
    }
    let locus = LocusClient::connect().await?;
    let association_locus = locus.clone();
    let associations = tokio::spawn(async move { associations::run(association_locus).await });
    let carbon_host = std::env::var("CARBON_HOST").unwrap_or_else(|_| "127.0.0.1".to_owned());
    let carbon_port: u16 = std::env::var("CARBON_PORT")
        .ok()
        .and_then(|port| port.parse().ok())
        .unwrap_or(2003);

    let metrics = Metrics::new(locus.clone(), carbon_host, carbon_port);
    let (shutdown, shutdown_signal) = tokio::sync::watch::channel(false);
    let mut metrics = tokio::spawn(async move { metrics.run(shutdown_signal).await });
    let namer = Namer::new(locus.clone());
    let namer = tokio::spawn(async move { namer.run().await });
    let git_status = GitStatus::new(locus.clone());
    let git_status = tokio::spawn(async move { git_status.run().await });

    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        biased;
        result = &mut metrics => report("metrics", result),
        result = namer => report("namer", result),
        result = git_status => report("git_status", result),
        result = associations => report("associations", result),
        _ = tokio::signal::ctrl_c() => {
            let _ = shutdown.send(true);
            report("metrics", metrics.await)
        },
        _ = terminate.recv() => {
            let _ = shutdown.send(true);
            report("metrics", metrics.await)
        },
    }
}

fn report(
    component: &'static str,
    result: Result<anyhow::Result<()>, tokio::task::JoinError>,
) -> anyhow::Result<()> {
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(anyhow::anyhow!("{component} failed: {error:#}")),
        Err(error) => Err(anyhow::anyhow!("{component} task failed: {error}")),
    }
}
