//! Pure project-management client. Desktop bindings belong to steward.
use clap::{Parser, Subcommand};
use proj_model::api::ManagerProxy;
use proj_model::*;
use std::path::PathBuf;
#[path = "cli/labels.rs"]
mod labels_cli;

#[derive(Parser)]
#[command(name = "proj", about = "Project-management client for projd", version)]
struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Permanently remove project metadata. Notebook files are untouched.
    #[command(alias = "trash")]
    Remove { id: String },
    /// Register a project directory (defaults to the current directory).
    Add {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    Root {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    Update {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    Refresh {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    Metadata {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        key: Option<String>,
    },
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    Checkout {
        #[command(subcommand)]
        command: CheckoutCommand,
    },
}
#[derive(Subcommand)]
enum ProjectCommand {
    /// Read or atomically edit project-owned labels.
    Labels {
        #[command(subcommand)]
        command: labels_cli::LabelsCommand,
    },
    Add {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    List {
        #[arg(long)]
        json: bool,
    },
    Rename {
        id: String,
        name: String,
    },
}
#[derive(Subcommand)]
enum CheckoutCommand {
    List {
        #[arg(long)]
        json: bool,
    },
}
fn absolute(path: PathBuf) -> anyhow::Result<String> {
    Ok(if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    }
    .to_string_lossy()
    .into_owned())
}
async fn register(proxy: &ManagerProxy<'_>, path: PathBuf) -> anyhow::Result<ProjectInfo> {
    Ok(proxy.register_project(&absolute(path)?).await?)
}
async fn resolve(proxy: &ManagerProxy<'_>, path: PathBuf) -> anyhow::Result<ProjectInfo> {
    Ok(proxy.resolve_project(&absolute(path)?).await?)
}
async fn add(proxy: &ManagerProxy<'_>, path: PathBuf, name: Option<String>) -> anyhow::Result<()> {
    let project = register(proxy, path).await?;
    if let Some(name) = name {
        let _: ProjectInfo = proxy.rename_project(&project.id, &name).await?;
    }
    println!("{}", project.id);
    Ok(())
}
async fn metadata(proxy: &ManagerProxy<'_>, path: PathBuf) -> anyhow::Result<serde_json::Value> {
    let p = resolve(proxy, path).await?;
    let labels = labels_cli::get(proxy, &p.id).await?;
    let checkouts: Vec<CheckoutInfo> = proxy.list_checkouts().await?;
    let c = checkouts.into_iter().find(|c| c.id == p.checkout_id);
    Ok(
        serde_json::json!({"id":p.id,"name":p.name,"cwd":p.cwd,"icon":p.icon,"icon-origin":p.icon_origin,"labels":labels,"path":c.as_ref().map(|c|c.root_path.as_str()).unwrap_or(&p.cwd),"checkout-id":p.checkout_id,"branch":c.as_ref().map(|c|c.branch.as_str()).unwrap_or(""),"checkout":c}),
    )
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let conn = zbus::Connection::session().await?;
    let proxy = ManagerProxy::new(&conn).await?;
    match args.command {
        Command::Remove { id } => {
            let removed: bool = proxy.remove_project(&id).await?;
            anyhow::ensure!(removed, "Project is not registered");
        }
        Command::Add { path, name } => add(&proxy, path, name).await?,
        Command::Root { path } => {
            let project = resolve(&proxy, path).await?;
            let checkouts: Vec<CheckoutInfo> = proxy.list_checkouts().await?;
            println!(
                "{}",
                checkouts
                    .into_iter()
                    .find(|c| c.id == project.checkout_id)
                    .map(|c| c.root_path)
                    .unwrap_or(project.cwd)
            );
        }
        Command::Update { path } | Command::Refresh { path } => {
            let project: ProjectInfo = proxy.refresh(&absolute(path)?).await?;
            println!("{}", serde_json::to_string(&project)?);
        }
        Command::Metadata { path, json, key } => {
            let value = metadata(&proxy, path).await?;
            if let Some(key) = key {
                let value = &value[&key];
                if value.is_string() {
                    println!("{}", value.as_str().unwrap());
                } else {
                    println!("{value}");
                }
            } else if json {
                println!("{value}");
            } else {
                println!("{}", serde_json::to_string_pretty(&value)?);
            }
        }
        Command::Project { command } => match command {
            ProjectCommand::Labels { command } => labels_cli::run(&proxy, command).await?,
            ProjectCommand::Add { path, name } => {
                add(&proxy, path, name).await?;
            }
            ProjectCommand::List { json } => {
                let rows: Vec<ProjectInfo> = proxy.list_projects().await?;
                if json {
                    println!("{}", serde_json::to_string(&rows)?);
                } else {
                    for p in rows {
                        println!("{}\t{}\t{}", p.id, p.name, p.cwd);
                    }
                }
            }
            ProjectCommand::Rename { id, name } => {
                let p: ProjectInfo = proxy.rename_project(&id, &name).await?;
                println!("{}", p.name);
            }
        },
        Command::Checkout {
            command: CheckoutCommand::List { json },
        } => {
            let rows: Vec<CheckoutInfo> = proxy.list_checkouts().await?;
            if json {
                println!("{}", serde_json::to_string(&rows)?);
            } else {
                for c in rows {
                    println!("{}\t{}\t{}", c.id, c.branch, c.root_path);
                }
            }
        }
    }
    Ok(())
}
