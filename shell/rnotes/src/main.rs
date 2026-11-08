//! `rnotes`: project tasks and Remarked notes for the Rsynapse shell.
//!
//! One client for the surface the shell already composes: projd project tasks
//! and Remarked notebooks. Project and task resolution happens here; the two
//! services stay independent and know nothing about each other.
mod args;
mod command;
mod output;
mod remarked;
mod scope;
mod tasknotes;

use anyhow::{Context, Result};
use clap::Parser;

use args::Cli;
use remarked::Notebook;

pub struct Session {
    pub connection: zbus::Connection,
    scope: notebook_api::notebook_scope::NotebookScope,
    bootstrap: std::sync::atomic::AtomicBool,
    notes: tokio::sync::OnceCell<Notebook>,
}

impl Session {
    /// Opens the scoped Remarked notebook on first use.
    pub async fn notes(&self) -> Result<&Notebook> {
        let notebook = self
            .notes
            .get_or_try_init(|| Notebook::open(&self.connection, self.scope.clone()))
            .await?;
        if self
            .bootstrap
            .swap(false, std::sync::atomic::Ordering::Relaxed)
            && self.scope.storage_path().is_some_and(needs_bootstrap)
        {
            notebook.bootstrap().await?;
        }
        Ok(notebook)
    }

    pub async fn status(
        connection: &zbus::Connection,
    ) -> Result<notebook_api::workspace::WorkspaceStatus> {
        Notebook::status(connection).await
    }

    pub async fn triage_policy(
        connection: &zbus::Connection,
    ) -> Result<notebook_api::triage::TriagePolicy> {
        Notebook::triage_policy(connection).await
    }

    pub async fn capabilities(&self) -> Result<notebook_api::Capabilities> {
        self.notes().await?.capabilities().await
    }
}

/// Remarked initializes a notebook with `zk init`, which fails if one already
/// exists. Only bootstrap an empty or uninitialized storage path.
fn needs_bootstrap(path: &str) -> bool {
    !std::path::Path::new(path).join(".zk").is_dir()
}

#[cfg(test)]
mod test;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let format = cli.format;
    if let Err(error) = run(cli).await {
        let error = output::classify(error);
        output::emit_error(format, &error);
        std::process::exit(i32::from(error.exit));
    }
}

async fn run(cli: Cli) -> Result<()> {
    let connection = zbus::Connection::session()
        .await
        .context("connect to the session bus")?;
    let resolved = scope::resolve(
        &connection,
        cli.project.as_deref(),
        cli.task.as_deref(),
        cli.notebook.as_deref(),
        cli.global,
        &cli.scope_tag,
    )
    .await?;
    let session = Session {
        connection,
        scope: resolved.scope.clone(),
        bootstrap: std::sync::atomic::AtomicBool::new(resolved.project.is_some()),
        notes: tokio::sync::OnceCell::new(),
    };
    let output = command::execute(cli.command, &session, &resolved).await?;
    output::emit(cli.format, cli.quiet, &resolved.view(), &output)
}
