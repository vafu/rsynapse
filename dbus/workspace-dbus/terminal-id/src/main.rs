use std::{fs::OpenOptions, io::Write, time::Duration};

use anyhow::{Context, Result, bail};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let Some(token) = args.next() else {
        bail!("usage: workspace-terminal-id FRESH-TOKEN [TTY]");
    };
    if token == "--help" || token == "-h" {
        println!(
            "workspace-terminal-id FRESH-TOKEN [TTY]\nIdentify a Ghostty window using an OSC title challenge and D-Bus signals.\nTTY defaults to /dev/tty; stdout contains only the opaque window ID."
        );
        return Ok(());
    }
    if token.is_empty() || token.chars().any(char::is_control) {
        bail!("title token must be nonempty and contain no control characters");
    }
    let tty = args.next().unwrap_or_else(|| "/dev/tty".into());
    if args.next().is_some() {
        bail!("too many arguments");
    }
    let mut tty = OpenOptions::new().write(true).open(tty)?;
    let result = tokio::time::timeout(Duration::from_secs(3), async {
        let connection = zbus::Connection::session().await?;
        workspace_terminal_id::resolve(&connection, &token, || async {
            write!(tty, "\x1b]2;{token}\x07")?;
            tty.flush()?;
            Ok(())
        })
        .await
    })
    .await
    .context("terminal title was not observed before discovery deadline")??;
    println!("{result}");
    Ok(())
}
