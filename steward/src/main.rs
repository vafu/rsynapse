mod focus;
mod git_status;
mod relations;
mod metrics;
mod namer;

use crate::relations::LocusClient;
use git_status::GitStatus;
use metrics::Metrics;
use namer::Namer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let locus = LocusClient::connect().await?;
    let carbon_host = std::env::var("CARBON_HOST").unwrap_or_else(|_| "127.0.0.1".to_owned());
    let carbon_port: u16 = std::env::var("CARBON_PORT")
        .ok()
        .and_then(|port| port.parse().ok())
        .unwrap_or(2003);

    let metrics = Metrics::new(locus.clone(), carbon_host, carbon_port);
    let metrics = tokio::spawn(async move { metrics.run().await });
    let namer = Namer::new(locus.clone());
    let namer = tokio::spawn(async move { namer.run().await });
    let git_status = GitStatus::new(locus.clone());
    let git_status = tokio::spawn(async move { git_status.run().await });

    tokio::select! {
        biased;
        result = metrics => report("metrics", result),
        result = namer => report("namer", result),
        result = git_status => report("git_status", result),
        _ = tokio::signal::ctrl_c() => Ok(()),
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
