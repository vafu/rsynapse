use anyhow::Result;
use clap::Parser;
use zbus::{Connection, proxy};

#[proxy(
    interface = "org.rsynapse.Launcher1",
    default_service = "com.rsynapse.Launcher",
    default_path = "/org/rsynapse/Launcher1"
)]
trait Launcher {
    async fn search(&self, query: &str) -> zbus::Result<Vec<String>>;
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The search term
    query: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let connection = Connection::session().await?;
    let proxy = LauncherProxy::new(&connection).await?;

    let results = proxy.search(&args.query).await?;

    if results.is_empty() {
        println!("No results found for '{}'", args.query);
    } else {
        println!("Results for '{}':", args.query);
        for item in results {
            println!("- {}", item);
        }
    }

    Ok(())
}
