use anyhow::Result;
use clap::{Parser, Subcommand};
use rsynapse_launcher_api::{EngineProxy, PluginScope, SearchRequest};
use tabled::{Table, Tabled};
use zbus::Connection;

#[derive(Tabled)]
struct ResultRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Title")]
    title: String,
    #[tabled(rename = "Description")]
    description: String,
    #[tabled(rename = "Data")]
    data: String,
}

#[derive(Tabled)]
struct PluginRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Aliases")]
    aliases: String,
    #[tabled(rename = "Scope")]
    scope: String,
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
        /// Launcher params, e.g. --param workspaces
        #[arg(long = "param", value_name = "PARAM")]
        params: Vec<String>,
        /// The search term
        query: String,
    },
    /// Execute an item by its ID.
    Exec {
        /// The ID of the item to execute
        id: String,
    },
    /// List launcher plugins and aliases.
    Describe,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let connection = Connection::session().await?;
    let proxy = EngineProxy::new(&connection).await?;

    match args.command {
        Commands::Search { params, query } => {
            let response = proxy
                .search(SearchRequest {
                    params,
                    query,
                    selected: String::new(),
                })
                .await?;
            if response.results.is_empty() {
                println!("No results found");
            } else {
                let table_data: Vec<ResultRow> = response
                    .results
                    .into_iter()
                    .map(|result| ResultRow {
                        id: result.id,
                        title: result.title,
                        description: result.description,
                        data: result.data,
                    })
                    .collect();
                println!("{}", Table::new(table_data));
            }
        }
        Commands::Exec { id } => {
            let result = proxy.execute(&id).await?;
            if !result.is_empty() {
                eprintln!("{}", result);
            }
        }
        Commands::Describe => {
            let response = proxy.describe().await?;
            let plugins = response
                .plugins
                .into_iter()
                .map(|plugin| PluginRow {
                    id: plugin.id,
                    name: plugin.name,
                    aliases: plugin.aliases.join(", "),
                    scope: match plugin.scope {
                        PluginScope::DirectOnly => "direct",
                        PluginScope::MainOnly => "main",
                        PluginScope::Both => "main+direct",
                    }
                    .to_owned(),
                })
                .collect::<Vec<_>>();
            println!("{}", Table::new(plugins));
        }
    }

    Ok(())
}
