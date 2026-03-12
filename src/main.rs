mod binary_cache;
mod cli;
mod commands;
mod config;
mod github;
mod installer;
mod package;
mod shim;
mod sync_dispatch;
mod ui;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands, LockAction};
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
        Commands::List { global } => commands::list::execute(&runtime, global)?,
        Commands::Update { package } => {
            commands::update::execute(&runtime, package.as_deref()).await?
        }
        Commands::Upgrade { package } => {
            commands::upgrade::execute(&runtime, package.as_deref()).await?
        }
        Commands::Reinstall {
            package,
            version,
            release,
            name,
            global,
        } => {
            commands::reinstall::execute(
                &runtime,
                &package,
                version.as_deref(),
                release.as_deref(),
                name.as_deref(),
                global,
            )
            .await?
        }
        Commands::Use { package, version } => {
            commands::use_version::execute(&runtime, &package, &version)?
        }
        Commands::Inspect { repo } => commands::inspect::execute(&runtime, &repo)?,
        Commands::Search { query } => commands::search::execute(&query).await?,
        Commands::Prebuild => commands::prebuild::execute(&runtime).await?,
        Commands::Why { package } => commands::why::execute(&runtime, &package)?,
        Commands::Lock { action } => match action {
            LockAction::Regenerate => commands::lock::regenerate(&runtime)?,
        },
        Commands::Doctor { fix } => commands::doctor::execute(&runtime, fix).await?,
        Commands::Config { action } => commands::config::execute(&runtime, action)?,
    }

    Ok(())
}
