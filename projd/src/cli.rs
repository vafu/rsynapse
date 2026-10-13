//! Pure project-management client. Desktop bindings belong to steward.
use clap::{Parser, Subcommand};
use proj_model::*;
use std::path::PathBuf;
use zbus::Proxy;
#[path = "cli/goals.rs"]
mod goal_cli;
use goal_cli::GoalCommand;

#[derive(Parser)]
#[command(name = "proj", about = "Project-management client for projd", version)]
struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Permanently remove project metadata. Files and goal history are untouched.
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
    #[command(alias = "task")]
    Goal {
        #[command(subcommand)]
        command: GoalCommand,
    },
}
#[derive(Subcommand)]
enum ProjectCommand {
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
async fn register(proxy: &Proxy<'_>, path: PathBuf) -> anyhow::Result<ProjectInfo> {
    Ok(proxy.call("RegisterProject", &(absolute(path)?,)).await?)
}
async fn resolve(proxy: &Proxy<'_>, path: PathBuf) -> anyhow::Result<ProjectInfo> {
    Ok(proxy.call("ResolveProject", &(absolute(path)?,)).await?)
}
async fn add(proxy: &Proxy<'_>, path: PathBuf, name: Option<String>) -> anyhow::Result<()> {
    let project = register(proxy, path).await?;
    if let Some(name) = name {
        let _: ProjectInfo = proxy.call("RenameProject", &(&project.id, name)).await?;
    }
    println!("{}", project.id);
    Ok(())
}
async fn metadata(proxy: &Proxy<'_>, path: PathBuf) -> anyhow::Result<serde_json::Value> {
    let p = resolve(proxy, path).await?;
    let checkouts: Vec<CheckoutInfo> = proxy.call("ListCheckouts", &()).await?;
    let c = checkouts.into_iter().find(|c| c.id == p.checkout_id);
    Ok(
        serde_json::json!({"id":p.id,"name":p.name,"cwd":p.cwd,"icon":p.icon,"icon-origin":p.icon_origin,"path":c.as_ref().map(|c|c.root_path.as_str()).unwrap_or(&p.cwd),"checkout-id":p.checkout_id,"branch":c.as_ref().map(|c|c.branch.as_str()).unwrap_or(""),"checkout":c}),
    )
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let conn = zbus::Connection::session().await?;
    let proxy = Proxy::new(&conn, BUS_NAME, ROOT_PATH, MANAGER_INTERFACE).await?;
    match args.command {
        Command::Remove { id } => {
            let removed: bool = proxy.call("RemoveProject", &(id,)).await?;
            anyhow::ensure!(removed, "Project is not registered");
        }
        Command::Add { path, name } => add(&proxy, path, name).await?,
        Command::Root { path } => {
            let project = resolve(&proxy, path).await?;
            let checkouts: Vec<CheckoutInfo> = proxy.call("ListCheckouts", &()).await?;
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
            let project: ProjectInfo = proxy.call("Refresh", &(absolute(path)?,)).await?;
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
            ProjectCommand::Add { path, name } => {
                add(&proxy, path, name).await?;
            }
            ProjectCommand::List { json } => {
                let rows: Vec<ProjectInfo> = proxy.call("ListProjects", &()).await?;
                if json {
                    println!("{}", serde_json::to_string(&rows)?);
                } else {
                    for p in rows {
                        println!("{}\t{}\t{}", p.id, p.name, p.cwd);
                    }
                }
            }
            ProjectCommand::Rename { id, name } => {
                let p: ProjectInfo = proxy.call("RenameProject", &(id, name)).await?;
                println!("{}", p.name);
            }
        },
        Command::Checkout {
            command: CheckoutCommand::List { json },
        } => {
            let rows: Vec<CheckoutInfo> = proxy.call("ListCheckouts", &()).await?;
            if json {
                println!("{}", serde_json::to_string(&rows)?);
            } else {
                for c in rows {
                    println!("{}\t{}\t{}", c.id, c.branch, c.root_path);
                }
            }
        }
        Command::Goal { command } => goal_cli::run(&proxy, command).await?,
    }
    Ok(())
}
