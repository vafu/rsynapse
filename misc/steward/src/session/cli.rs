use super::{
    BUS, INTERFACE, ROOT,
    model::{RestoreReport, Session},
};
use clap::{Parser, Subcommand};
use futures_util::StreamExt;

#[derive(Parser)]
#[command(
    bin_name = "rsynapse-steward session",
    about = "Save and restore project-aware desktop sessions"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    List,
    Show {
        #[arg(default_value = "last")]
        name: String,
    },
    Save {
        #[arg(default_value = "last")]
        name: String,
    },
    Restore {
        #[arg(default_value = "last")]
        name: String,
    },
}
pub async fn run() -> anyhow::Result<bool> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_none_or(|a| a != "session") {
        return Ok(false);
    }
    let args = Args::parse_from(std::iter::once(args[0].clone()).chain(args.into_iter().skip(2)));
    let connection = zbus::Connection::session().await?;
    let proxy = zbus::Proxy::new(&connection, BUS, ROOT, INTERFACE).await?;
    let save = matches!(&args.command, Command::Save { .. });
    match args.command {
        Command::List => {
            let sessions: Vec<String> = proxy.call("List", &()).await?;
            println!("{}", serde_json::to_string_pretty(&sessions)?);
        }
        Command::Save { name } | Command::Show { name } => {
            let method = if save { "Save" } else { "Get" };
            let session: Session = proxy.call(method, &(name,)).await?;
            println!("{}", serde_json::to_string_pretty(&session)?);
        }
        Command::Restore { name } => {
            let mut changed = proxy.receive_signal("RestoreChanged").await?;
            let job: String = proxy.call("Restore", &(name,)).await?;
            loop {
                let report: RestoreReport = proxy.call("GetRestore", &(job.clone(),)).await?;
                if report.state != "running" {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                    anyhow::ensure!(
                        report.issues.is_empty(),
                        "session restoration was partial; see reported issues"
                    );
                    break;
                }
                let message = changed
                    .next()
                    .await
                    .ok_or_else(|| anyhow::anyhow!("restore status stream ended"))?;
                let (identity, _): (String, RestoreReport) = message.body().deserialize()?;
                if identity != job {
                    continue;
                }
            }
        }
    }
    Ok(true)
}
