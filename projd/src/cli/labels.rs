use super::resolve;
use clap::{Args, Subcommand};
use proj_model::{BUS_NAME, PROJECT_INTERFACE, object_path};
use std::path::PathBuf;
use zbus::Proxy;

#[derive(Args)]
pub struct Target {
    /// Resolve an existing project by directory (defaults to the current directory).
    #[arg(long, conflicts_with = "project_id")]
    project: Option<PathBuf>,
    /// Address a registered project directly by UUID.
    #[arg(long)]
    project_id: Option<String>,
    #[arg(long)]
    json: bool,
}

#[derive(Subcommand)]
pub enum LabelsCommand {
    /// Read the project's labels.
    Get(Target),
    /// Atomically add labels, preserving spelling and internal spaces.
    Add {
        #[command(flatten)]
        target: Target,
        #[arg(required = true, num_args = 1..)]
        labels: Vec<String>,
    },
    /// Atomically remove exact labels.
    Remove {
        #[command(flatten)]
        target: Target,
        #[arg(required = true, num_args = 1..)]
        labels: Vec<String>,
    },
}

pub async fn get(proxy: &Proxy<'_>, id: &str) -> anyhow::Result<Vec<String>> {
    let project = Proxy::new(
        proxy.connection(),
        BUS_NAME,
        object_path("Projects", id),
        PROJECT_INTERFACE,
    )
    .await?;
    Ok(project.get_property("Labels").await?)
}

pub async fn run(proxy: &Proxy<'_>, command: LabelsCommand) -> anyhow::Result<()> {
    let (target, method, labels) = match command {
        LabelsCommand::Get(target) => (target, None, Vec::new()),
        LabelsCommand::Add { target, labels } => (target, Some("AddProjectLabels"), labels),
        LabelsCommand::Remove { target, labels } => (target, Some("RemoveProjectLabels"), labels),
    };
    let id = match target.project_id {
        Some(id) => id,
        None => {
            resolve(proxy, target.project.unwrap_or_else(|| ".".into()))
                .await?
                .id
        }
    };
    let result: Vec<String> = match method {
        Some(method) => proxy.call(method, &(id, labels)).await?,
        None => get(proxy, &id).await?,
    };
    if target.json {
        println!("{}", serde_json::to_string(&result)?);
    } else {
        for label in result {
            println!("{label}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::Args;
    use clap::Parser;

    #[test]
    fn label_operand_and_target_validation() {
        for op in ["add", "remove"] {
            assert!(Args::try_parse_from(["proj", "project", "labels", op]).is_err());
            assert!(
                Args::try_parse_from([
                    "proj",
                    "project",
                    "labels",
                    op,
                    "bug",
                    "team name",
                    "--json",
                    "--project-id",
                    "id"
                ])
                .is_ok()
            );
        }
        assert!(
            Args::try_parse_from([
                "proj",
                "project",
                "labels",
                "get",
                "--project",
                "/tmp",
                "--project-id",
                "id"
            ])
            .is_err()
        );
        assert!(Args::try_parse_from(["proj", "project", "labels", "get"]).is_ok());
    }
}
