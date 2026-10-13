use clap::{Args, Subcommand};
use proj_model::{Goal, GoalInfo, ProjectInfo};
use std::path::PathBuf;
use zbus::Proxy;

#[derive(Args)]
pub struct NewGoal {
    /// Optional stable ID; otherwise a UUID is generated.
    id: Option<String>,
    #[arg(long)]
    title: String,
    #[arg(long)]
    description: Option<String>,
    #[arg(long)]
    parent: Option<String>,
    #[arg(long, conflicts_with = "parent")]
    project: Option<PathBuf>,
    #[arg(long, conflicts_with_all=["project", "parent"])]
    global: bool,
    #[arg(long)]
    active: bool,
    #[arg(long, default_value="task", value_parser=["task","outcome","habit"])]
    kind: String,
    #[arg(long, default_value = "")]
    success: String,
    #[arg(long, default_value="medium", value_parser=["low","medium","high"])]
    priority: String,
    #[arg(long)]
    date: Option<String>,
}
#[derive(Subcommand)]
pub enum GoalCommand {
    List {
        #[arg(long)]
        date: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        project: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    Add(NewGoal),
    /// Create a named task, or update its title/description if it exists.
    Set(NewGoal),
    Update {
        id: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        date: Option<String>,
    },
    Show {
        id: String,
        #[arg(long)]
        date: Option<String>,
    },
    Status {
        id: String,
        #[arg(value_parser=["planned","todo","in-progress","completed","deferred"])]
        status: String,
        #[arg(long)]
        date: Option<String>,
    },
    Complete {
        id: String,
        #[arg(long)]
        date: Option<String>,
    },
    /// Mark in progress and select as the project's active task.
    Start {
        id: String,
        #[arg(long)]
        date: Option<String>,
    },
    Active {
        id: Option<String>,
        #[arg(long)]
        clear: bool,
        #[arg(long)]
        project: Option<PathBuf>,
    },
    Note {
        id: String,
        #[arg(long)]
        date: Option<String>,
        #[arg(long)]
        open: bool,
    },
    Remove {
        id: String,
        #[arg(long)]
        date: Option<String>,
    },
    Import {
        file: PathBuf,
    },
}

async fn rows(proxy: &Proxy<'_>) -> anyhow::Result<Vec<GoalInfo>> {
    Ok(proxy.call("ListGoals", &(String::new(),)).await?)
}
fn find<'a>(rows: &'a [GoalInfo], id: &str, date: Option<&str>) -> anyhow::Result<&'a GoalInfo> {
    let matches: Vec<_> = rows
        .iter()
        .filter(|g| (g.key() == id || g.id == id) && date.is_none_or(|d| g.date == d))
        .collect();
    anyhow::ensure!(
        matches.len() == 1,
        "Goal not found or ID is ambiguous; use its full date/ID key"
    );
    Ok(matches[0])
}
async fn project(proxy: &Proxy<'_>, path: Option<PathBuf>) -> anyhow::Result<ProjectInfo> {
    Ok(proxy
        .call(
            "ResolveProject",
            &(super::absolute(path.unwrap_or_else(|| PathBuf::from(".")))?,),
        )
        .await?)
}
async fn select(proxy: &Proxy<'_>, goal: &GoalInfo) -> anyhow::Result<()> {
    anyhow::ensure!(
        !goal.project_id.is_empty(),
        "Task has no registered project"
    );
    Ok(proxy
        .call("SetActiveGoal", &(&goal.project_id, goal.key()))
        .await?)
}

pub async fn run(proxy: &Proxy<'_>, command: GoalCommand) -> anyhow::Result<()> {
    match command {
        GoalCommand::List {
            date,
            all,
            project: path,
            json,
        } => {
            let mut rows = rows(proxy).await?;
            if let Some(date) = date {
                rows.retain(|g| g.date == date);
            } else if !all {
                let p = project(proxy, path).await?;
                rows.retain(|g| g.project_id == p.id);
            }
            let goals = rows
                .into_iter()
                .map(Goal::try_from)
                .collect::<Result<Vec<_>, _>>()
                .map_err(anyhow::Error::msg)?;
            if json {
                println!("{}", serde_json::to_string(&goals)?);
            } else {
                for g in goals {
                    println!(
                        "{} [{}] {}{}",
                        g.key(),
                        serde_json::to_value(g.status)?.as_str().unwrap(),
                        if g.parent_id.is_empty() { "" } else { "  ↳ " },
                        g.title
                    );
                }
            }
        }
        GoalCommand::Add(new) => create(proxy, new, false).await?,
        GoalCommand::Set(new) => create(proxy, new, true).await?,
        GoalCommand::Update {
            id,
            title,
            description,
            date,
        } => {
            let rows = rows(proxy).await?;
            let goal = find(&rows, &id, date.as_deref())?;
            let mut fields = std::collections::HashMap::new();
            if let Some(title) = title {
                fields.insert("title", title);
            }
            if let Some(description) = description {
                fields.insert("description", description);
            }
            let result: GoalInfo = proxy.call("EditGoal", &(goal.key(), fields)).await?;
            println!("{}", result.key());
        }
        GoalCommand::Show { id, date } => {
            let rows = rows(proxy).await?;
            let goal: Goal = find(&rows, &id, date.as_deref())?
                .clone()
                .try_into()
                .map_err(anyhow::Error::msg)?;
            println!("{}", serde_json::to_string_pretty(&goal)?);
        }
        GoalCommand::Status { id, status, date } => {
            set_status(proxy, &id, &status, date.as_deref(), false).await?
        }
        GoalCommand::Complete { id, date } => {
            set_status(proxy, &id, "completed", date.as_deref(), false).await?
        }
        GoalCommand::Start { id, date } => {
            set_status(proxy, &id, "in-progress", date.as_deref(), true).await?
        }
        GoalCommand::Active {
            id,
            clear,
            project: path,
        } => {
            anyhow::ensure!(!(clear && id.is_some()), "Use a task ID or --clear");
            if let Some(id) = id {
                let rows = rows(proxy).await?;
                select(proxy, find(&rows, &id, None)?).await?;
            } else {
                let p = project(proxy, path).await?;
                if clear {
                    let _: () = proxy.call("SetActiveGoal", &(p.id, String::new())).await?;
                } else {
                    let conn = zbus::Connection::session().await?;
                    let obj = zbus::Proxy::new(
                        &conn,
                        proj_model::BUS_NAME,
                        proj_model::object_path("Projects", &p.id),
                        proj_model::PROJECT_INTERFACE,
                    )
                    .await?;
                    let active: zbus::zvariant::OwnedObjectPath =
                        obj.get_property("ActiveGoal").await?;
                    let rows = rows(proxy).await?;
                    if let Some(g) = rows.iter().find(|g| g.object_path() == active.as_str()) {
                        println!("{}", serde_json::to_string(g)?);
                    } else {
                        println!("null");
                    }
                }
            }
        }
        GoalCommand::Note { id, date, open } => {
            let rows = rows(proxy).await?;
            let goal = find(&rows, &id, date.as_deref())?;
            if open {
                let editor = std::env::var("EDITOR")
                    .or_else(|_| std::env::var("VISUAL"))
                    .unwrap_or_else(|_| "vi".into());
                let args = shell_words::split(&editor)?;
                anyhow::ensure!(!args.is_empty(), "EDITOR is empty");
                let status = std::process::Command::new(&args[0])
                    .args(&args[1..])
                    .arg(&goal.note_path)
                    .status()?;
                anyhow::ensure!(status.success(), "Editor failed");
            } else {
                println!("{}", goal.note_path);
            }
        }
        GoalCommand::Remove { id, date } => {
            let rows = rows(proxy).await?;
            let goal = find(&rows, &id, date.as_deref())?;
            let removed: bool = proxy.call("RemoveGoal", &(&goal.date, &goal.id)).await?;
            anyhow::ensure!(removed, "Goal not found");
        }
        GoalCommand::Import { file } => {
            let goals: Vec<Goal> = serde_json::from_slice(&std::fs::read(file)?)?;
            for goal in goals {
                goal.validate().map_err(anyhow::Error::msg)?;
                let info: GoalInfo = goal.into();
                if rows(proxy).await?.iter().any(|g| g.key() == info.key()) {
                    continue;
                }
                let _: GoalInfo = proxy.call("CreateGoal", &(info,)).await?;
            }
        }
    }
    Ok(())
}

async fn create(proxy: &Proxy<'_>, new: NewGoal, upsert: bool) -> anyhow::Result<()> {
    let all = rows(proxy).await?;
    if upsert {
        anyhow::ensure!(new.id.is_some(), "goal set requires a stable ID");
        if all.iter().any(|g| {
            (g.id == *new.id.as_ref().unwrap() || g.key() == *new.id.as_ref().unwrap())
                && new.date.as_ref().is_none_or(|d| g.date == *d)
        }) {
            let existing = find(&all, new.id.as_deref().unwrap(), new.date.as_deref())?;
            anyhow::ensure!(
                !new.active
                    || (!existing.project_id.is_empty()
                        && !matches!(existing.status.as_str(), "completed" | "deferred")),
                "Only an open task in a registered project can be tracked"
            );
            let mut fields = std::collections::HashMap::from([("title", new.title)]);
            if let Some(description) = new.description {
                fields.insert("description", description);
            }
            let result: GoalInfo = proxy.call("EditGoal", &(existing.key(), fields)).await?;
            if new.active {
                select(proxy, &result).await?;
            }
            println!("{}", result.key());
            return Ok(());
        }
    }
    let parent = new
        .parent
        .as_deref()
        .map(|id| find(&all, id, None))
        .transpose()?;
    let path = if let Some(parent) = parent {
        parent.project.clone()
    } else if new.global || (new.kind == "habit" && new.project.is_none()) {
        String::new()
    } else if let Some(path) = new.project {
        super::absolute(path)?
    } else {
        project(proxy, None).await?.cwd
    };
    let goal = GoalInfo {
        id: new
            .id
            .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string()),
        date: new.date.unwrap_or_default(),
        title: new.title,
        description: new.description.unwrap_or_default(),
        parent_id: parent.map(|p| p.key()).unwrap_or_default(),
        project: path,
        kind: new.kind,
        success: new.success,
        priority: new.priority,
        status: "planned".into(),
        ..Default::default()
    };
    if new.active {
        anyhow::ensure!(
            !goal.project.is_empty(),
            "An active task requires a registered project"
        );
        let _: ProjectInfo = proxy.call("ResolveProject", &(&goal.project,)).await?;
    }
    let result: GoalInfo = proxy.call("CreateGoal", &(goal,)).await?;
    if new.active {
        select(proxy, &result).await?;
    }
    println!("{}", result.key());
    Ok(())
}
async fn set_status(
    proxy: &Proxy<'_>,
    id: &str,
    status: &str,
    date: Option<&str>,
    active: bool,
) -> anyhow::Result<()> {
    let rows = rows(proxy).await?;
    let goal = find(&rows, id, date)?;
    if active {
        anyhow::ensure!(
            !goal.project_id.is_empty(),
            "Task has no registered project"
        );
    }
    let result: GoalInfo = proxy
        .call(
            "SetGoalStatus",
            &(
                goal.key(),
                if status == "todo" { "planned" } else { status },
            ),
        )
        .await?;
    if active {
        select(proxy, &result).await?;
    }
    Ok(())
}
