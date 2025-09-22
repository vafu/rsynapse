use anyhow::Result;
use clap::{Parser, Subcommand};
use tabled::{Table, Tabled};
use zbus::{Connection, proxy};

#[proxy(
    interface = "org.rsynapse.Engine1",
    default_service = "com.rsynapse.Engine",
    default_path = "/org/rsynapse/Engine1"
)]
trait Engine {
    async fn search(
        &self,
        query: &str,
    ) -> zbus::Result<Vec<(String, String, String, String, String)>>;
}

#[derive(Tabled)]
struct ResultRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Title")]
    title: String,
    #[tabled(rename = "Description")]
    description: String,
    #[tabled(rename = "Command")]
    command: String,
}

#[derive(Parser, Debug)]
#[command(version, about = "A command-line interface for the rsynapse daemon.")]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Search for an item.
    Search {
        /// The search term
        query: String,
    },
    /// Execute an item by its ID.
    Exec {
        /// The ID of the item to execute
        id: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let connection = Connection::session().await?;
    let proxy = EngineProxy::new(&connection).await?;

    match args.command {
        Commands::Search { query } => {
            let results = proxy.search(&query).await?;
            if results.is_empty() {
                println!("No results found for '{}'", query);
            } else {
                let table_data: Vec<ResultRow> = results
                    .into_iter()
                    .map(|(id, title, description, _icon, command)| ResultRow {
                        id,
                        title,
                        description,
                        command,
                    })
                    .collect();
                println!("{}", Table::new(table_data));
            }
        }
        Commands::Exec { id } => {
            // TODO: add execute
            println!("Execution request sent for ID: {}", id);
        }
    }

    Ok(())
}
