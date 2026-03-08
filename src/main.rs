mod cli;
mod commands;
mod config;
mod github;
mod installer;
mod package;
mod shim;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use config::RuntimeContext;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let runtime = RuntimeContext::load_or_init()?;

    match cli.command {
        Commands::Sync {
            repo,
            version,
            release,
            name,
            global,
        } => {
            commands::sync::execute(
                &runtime,
                &repo,
                version.as_deref(),
                release.as_deref(),
                name.as_deref(),
                global,
            )
            .await?
        }
        Commands::Remove { repo } => commands::remove::execute(&runtime, &repo)?,
        Commands::Info { package } => commands::info::execute(&runtime, &package)?,
        Commands::Outdated => commands::outdated::execute(&runtime)?,
        Commands::Clean { repos } => commands::clean::execute(&runtime, repos)?,
        Commands::Exec { repo, args } => commands::exec::execute(&runtime, &repo, &args).await?,
        Commands::Which { command } => commands::which::execute(&runtime, &command)?,
        Commands::Run { package, args } => {
            commands::run::execute(&runtime, &package, &args).await?
        }
        Commands::List => commands::list::execute(&runtime)?,
        Commands::Update { package } => {
            commands::update::execute(&runtime, package.as_deref()).await?
        }
        Commands::Doctor => commands::doctor::execute(&runtime)?,
        Commands::Config { action } => commands::config::execute(&runtime, action)?,
    }

    Ok(())
}
