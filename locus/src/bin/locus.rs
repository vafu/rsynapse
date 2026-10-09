//! CLI for locusd's org.rsynapse.Locus relation service.
use anyhow::{Context, bail};
use locus::{RelationEndpoint, RelationsProxy};

fn boolean(value: &str) -> anyhow::Result<bool> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => bail!("persist must be true or false"),
    }
}

fn endpoint(value: &str) -> anyhow::Result<RelationEndpoint> {
    serde_json::from_str(value).context("endpoint must be a JSON endpoint dictionary")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let usage = "locus list [RELATION]\n\
                 locus set|set-one SUBJECT_JSON RELATION TARGET_JSON METADATA_JSON [--persist true|false]\n\
                 locus persist SUBJECT_JSON RELATION TARGET_JSON true|false\n\
                 CLI for locusd (org.rsynapse.Locus). New records default to persist false.";
    if args.is_empty() || matches!(args[0].as_str(), "--help" | "-h") {
        println!("{usage}");
        return Ok(());
    }
    // Validate command shape before connecting; help never activates the service.
    match args[0].as_str() {
        "list" if args.len() <= 2 => {}
        "persist" if args.len() == 5 => {
            boolean(&args[4])?;
        }
        "set" | "set-one" if args.len() == 5 || args.len() == 7 => {
            if args.len() == 7 {
                if args[5] != "--persist" {
                    bail!("{usage}");
                }
                boolean(&args[6])?;
            }
        }
        _ => bail!("{usage}"),
    }
    let connection = zbus::Connection::session().await?;
    let proxy = RelationsProxy::new(&connection).await?;
    let output = match args[0].as_str() {
        "list" => serde_json::to_value(
            proxy
                .list_with_persistence(args.get(1).map(String::as_str).unwrap_or(""))
                .await?,
        )?,
        "persist" => serde_json::to_value(
            proxy
                .set_persistence(
                    endpoint(&args[1])?,
                    &args[2],
                    endpoint(&args[3])?,
                    boolean(&args[4])?,
                )
                .await?,
        )?,
        command => {
            let subject = endpoint(&args[1])?;
            let target = endpoint(&args[3])?;
            let metadata = serde_json::from_str(&args[4])
                .context("metadata must be a JSON string dictionary")?;
            let persist = if args.len() == 7 {
                boolean(&args[6])?
            } else {
                false
            };
            let state = if command == "set" {
                proxy
                    .set_with_persistence(subject, &args[2], target, metadata, persist)
                    .await?
            } else {
                proxy
                    .set_one_with_persistence(subject, &args[2], target, metadata, persist)
                    .await?
            };
            serde_json::to_value(state)?
        }
    };
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
