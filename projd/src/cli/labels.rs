use super::resolve;
use clap::{Args, Subcommand};
use proj_model::api::ManagerProxy;
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

pub async fn get(proxy: &ManagerProxy<'_>, id: &str) -> anyhow::Result<Vec<String>> {
    let project = Proxy::new(
        proxy.inner().connection(),
        BUS_NAME,
        object_path("Projects", id),
        PROJECT_INTERFACE,
    )
    .await?;
    Ok(project.get_property("Labels").await?)
}

pub async fn run(proxy: &ManagerProxy<'_>, command: LabelsCommand) -> anyhow::Result<()> {
    let (target, action, labels) = match command {
        LabelsCommand::Get(target) => (target, None, Vec::new()),
        LabelsCommand::Add { target, labels } => (target, Some(true), labels),
        LabelsCommand::Remove { target, labels } => (target, Some(false), labels),
    };
    let id = match target.project_id {
        Some(id) => id,
        None => {
            resolve(proxy, target.project.unwrap_or_else(|| ".".into()))
                .await?
                .id
        }
    };
    let result: Vec<String> = match action {
        Some(true) => proxy.add_project_labels(&id, labels).await?,
        Some(false) => proxy.remove_project_labels(&id, labels).await?,
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
