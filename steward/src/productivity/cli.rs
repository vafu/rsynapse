//! Activity-context preferences only. Goal management belongs to Grafana.
use super::{CLASSIFICATION, Classification, WORKSPACE_CONTEXT};
use crate::relations::LocusClient;
use clap::{Parser, Subcommand};
use locus::RelationEndpoint;
use std::{collections::HashMap, path::PathBuf};

#[derive(Parser)]
#[command(
    name = "steward",
    about = "Session steward and activity-context preferences",
    after_help = "Manage projects and daily goals with proj, or Grafana's Rsynapse goals panel."
)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    Classify {
        #[command(subcommand)]
        command: Classify,
    },
    BindCurrent {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    Associations,
}
#[derive(Subcommand)]
enum Classify {
    Workspace {
        name: String,
        #[arg(value_enum)]
        class: Classification,
    },
    Project {
        path: PathBuf,
        #[arg(value_enum)]
        class: Classification,
    },
    List,
}
pub async fn run() -> anyhow::Result<bool> {
    let Some(command) = Args::parse().command else {
        return Ok(false);
    };
    let locus = LocusClient::connect().await?;
    let command = match command {
        Command::BindCurrent { path } => {
            let path = path.canonicalize()?;
            let connection = zbus::Connection::session().await?;
            let proxy = zbus::Proxy::new(
                &connection,
                "org.rsynapse.Steward",
                "/org/rsynapse/Steward",
                "org.rsynapse.Steward.Associations1",
            )
            .await?;
            let _: goal_model::ProjectInfo = proxy
                .call(
                    "BindCurrentProject",
                    &(path.to_string_lossy().into_owned(),),
                )
                .await?;
            return Ok(true);
        }
        Command::Associations => {
            crate::associations::run(locus).await?;
            return Ok(true);
        }
        Command::Classify { command } => command,
    };
    let (kind, id, class) = match command {
        Classify::Workspace { name, class } => {
            anyhow::ensure!(!name.trim().is_empty(), "Workspace context cannot be empty");
            (WORKSPACE_CONTEXT, name, class)
        }
        Classify::Project { path, class } => {
            let path = path.canonicalize()?;
            anyhow::ensure!(path.is_dir(), "Project must be a directory");
            (
                locus::keys::PROJECT_PATH,
                path.to_string_lossy().into_owned(),
                class,
            )
        }
        Classify::List => {
            for record in locus.list(CLASSIFICATION).await? {
                println!(
                    "{:?}\t{}",
                    record.subject,
                    record
                        .metadata
                        .get("class")
                        .map(String::as_str)
                        .unwrap_or("unknown")
                );
            }
            return Ok(true);
        }
    };
    let label = serde_json::to_value(class)?.as_str().unwrap().to_owned();
    locus
        .set_one_with_persistence(
            RelationEndpoint::stable_key(kind, id),
            CLASSIFICATION,
            RelationEndpoint::stable_key("org.rsynapse.classification", label),
            HashMap::from([("class".to_owned(), serde_json::to_string(&class)?)]),
            true,
        )
        .await?;
    Ok(true)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collector_has_context_preferences_but_no_goal_management() {
        assert!(
            Args::try_parse_from(["steward", "classify", "workspace", "personal", "personal"])
                .is_ok()
        );
        assert!(Args::try_parse_from(["steward", "goal", "list"]).is_err());
    }
}
