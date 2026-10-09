//! Pure project-management client. Desktop bindings belong to steward.
use clap::{Parser, Subcommand};
use proj_model::*;
use std::path::PathBuf;
use zbus::Proxy;

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
#[derive(Subcommand)]
enum GoalCommand {
    Import {
        file: PathBuf,
    },
    List {
        #[arg(long)]
        date: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
    },
    Add {
        id: String,
        #[arg(long)]
        title: String,
        #[arg(long,default_value="outcome",value_parser=["outcome","habit"])]
        kind: String,
        #[arg(long)]
        project: Option<PathBuf>,
        #[arg(long)]
        success: String,
        #[arg(long,default_value="medium",value_parser=["low","medium","high"])]
        priority: String,
        #[arg(long)]
        date: Option<String>,
    },
    Status {
        id: String,
        #[arg(value_parser=["planned","in-progress","completed","deferred"])]
        status: String,
        #[arg(long)]
        date: Option<String>,
    },
    Remove {
        id: String,
        #[arg(long)]
        date: Option<String>,
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
fn day(date: Option<String>) -> anyhow::Result<String> {
    let day = date.unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());
    anyhow::ensure!(valid_date(&day), "Date must be YYYY-MM-DD");
    Ok(day)
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
        Command::Goal { command } => match command {
            GoalCommand::Import { file } => {
                let goals: Vec<Goal> = serde_json::from_slice(&std::fs::read(file)?)?;
                for goal in goals {
                    goal.validate().map_err(anyhow::Error::msg)?;
                    let info: GoalInfo = goal.into();
                    let existing: Vec<GoalInfo> = proxy.call("ListGoals", &(&info.date,)).await?;
                    if existing.iter().any(|g| g.id == info.id) {
                        continue;
                    }
                    let _: GoalInfo = proxy.call("CreateGoal", &(info,)).await?;
                }
            }
            GoalCommand::List { date, all, json } => {
                let rows: Vec<GoalInfo> = proxy
                    .call(
                        "ListGoals",
                        &(if all { String::new() } else { day(date)? },),
                    )
                    .await?;
                let goals: Vec<Goal> = rows
                    .into_iter()
                    .map(Goal::try_from)
                    .collect::<Result<_, _>>()
                    .map_err(anyhow::Error::msg)?;
                if json {
                    println!("{}", serde_json::to_string(&goals)?);
                } else {
                    for g in goals {
                        println!(
                            "{} {} {:?} {:?}\n  {}\n  Success: {}",
                            g.date, g.id, g.kind, g.status, g.title, g.success
                        );
                    }
                }
            }
            GoalCommand::Add {
                id,
                title,
                kind,
                project,
                success,
                priority,
                date,
            } => {
                let info = GoalInfo {
                    id,
                    date: day(date)?,
                    title,
                    kind,
                    project: project.map(absolute).transpose()?.unwrap_or_default(),
                    success,
                    priority,
                    status: "planned".into(),
                };
                let info: GoalInfo = proxy.call("CreateGoal", &(info,)).await?;
                println!("{}/{}", info.date, info.id);
            }
            GoalCommand::Status { id, status, date } => {
                let date = day(date)?;
                let rows: Vec<GoalInfo> = proxy.call("ListGoals", &(&date,)).await?;
                let mut goal = rows
                    .into_iter()
                    .find(|g| g.id == id)
                    .ok_or_else(|| anyhow::anyhow!("Goal not found"))?;
                goal.status = status;
                let _: GoalInfo = proxy.call("UpdateGoal", &(goal,)).await?;
            }
            GoalCommand::Remove { id, date } => {
                let removed: bool = proxy.call("RemoveGoal", &(day(date)?, id)).await?;
                anyhow::ensure!(removed, "Goal not found");
            }
        },
    }
    Ok(())
}
