use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "mntpack", version, about = "mntpack Package Manager")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[command(alias = "add", alias = "install")]
    Sync {
        repo: String,
        #[arg(short = 'v', long = "version")]
        version: Option<String>,
        #[arg(short = 'r', long = "release")]
        release: Option<String>,
        #[arg(short = 'n', long = "name")]
        name: Option<String>,
        #[arg(short = 'g', long = "global")]
        global: bool,
    },
    #[command(alias = "uninstall", alias = "rm", alias = "unsync")]
    Remove {
        repo: String,
    },
    Info {
        package: String,
    },
    Outdated,
    Clean {
        #[arg(long = "repos")]
        repos: bool,
    },
    Exec {
        repo: String,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    Which {
        command: String,
    },
    #[command(disable_help_flag = true)]
    Run {
        package: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    List {
        #[arg(short = 'g', long = "global")]
        global: bool,
    },
    Update {
        package: Option<String>,
    },
    Upgrade {
        package: Option<String>,
    },
    #[command(alias = "resync")]
    Reinstall {
        package: String,
        #[arg(short = 'v', long = "version")]
        version: Option<String>,
        #[arg(short = 'r', long = "release")]
        release: Option<String>,
        #[arg(short = 'n', long = "name")]
        name: Option<String>,
        #[arg(short = 'g', long = "global")]
        global: bool,
    },
    Use {
        package: String,
        version: String,
    },
    Inspect {
        repo: String,
    },
    Search {
        #[arg(required = true)]
        query: Vec<String>,
    },
    Prebuild,
    Why {
        package: String,
    },
    Doctor {
        #[arg(short = 'f', long = "fix")]
        fix: bool,
    },
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    Nuget {
        #[command(subcommand)]
        action: NugetAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    Show,
    Get { key: String },
    Set { key: String, value: String },
    Reset,
}

#[derive(Debug, Subcommand)]
pub enum NugetAction {
    #[command(alias = "ensure")]
    Init {
        #[arg(long = "path")]
        path: Option<PathBuf>,
        #[arg(long = "project")]
        project: Option<PathBuf>,
    },
    Feed {
        #[command(subcommand)]
        action: NugetFeedAction,
    },
    Cache {
        #[command(subcommand)]
        action: NugetCacheAction,
    },
    Source {
        #[command(subcommand)]
        action: NugetSourceAction,
    },
    Add {
        package: String,
        version: Option<String>,
        #[arg(long = "source")]
        source: Option<String>,
        #[arg(long = "path")]
        path: Option<PathBuf>,
        #[arg(long = "project")]
        project: Option<PathBuf>,
        #[arg(long = "no-restore")]
        no_restore: bool,
        #[arg(long = "refresh")]
        refresh: bool,
        #[arg(long = "build")]
        build: bool,
    },
    Use {
        package: String,
        version: Option<String>,
        #[arg(long = "source")]
        source: Option<String>,
        #[arg(long = "path")]
        path: Option<PathBuf>,
        #[arg(long = "project")]
        project: Option<PathBuf>,
        #[arg(long = "no-restore")]
        no_restore: bool,
        #[arg(long = "refresh")]
        refresh: bool,
        #[arg(long = "build")]
        build: bool,
    },
    Remove {
        package: String,
        #[arg(long = "path")]
        path: Option<PathBuf>,
        #[arg(long = "project")]
        project: Option<PathBuf>,
        #[arg(long = "no-restore")]
        no_restore: bool,
        #[arg(long = "build")]
        build: bool,
    },
    List {
        #[arg(long = "path")]
        path: Option<PathBuf>,
        #[arg(long = "project")]
        project: Option<PathBuf>,
    },
    #[command(alias = "install")]
    Apply {
        #[arg(long = "path")]
        path: Option<PathBuf>,
        #[arg(long = "project")]
        project: Option<PathBuf>,
        #[arg(long = "refresh")]
        refresh: bool,
        #[arg(long = "build")]
        build: bool,
    },
    Restore {
        #[arg(long = "path")]
        path: Option<PathBuf>,
        #[arg(long = "project")]
        project: Option<PathBuf>,
        #[arg(long = "refresh")]
        refresh: bool,
        #[arg(long = "build")]
        build: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum NugetFeedAction {
    Path,
    List,
}

#[derive(Debug, Subcommand)]
pub enum NugetCacheAction {
    Clear {
        package: String,
        version: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum NugetSourceAction {
    Add {
        name: String,
        #[arg(long = "repo")]
        repo: String,
        #[arg(long = "ref")]
        reference: Option<String>,
        #[arg(long = "subdir")]
        subdir: Option<PathBuf>,
        #[arg(long = "project")]
        project: Option<PathBuf>,
        #[arg(long = "solution")]
        solution: Option<PathBuf>,
        #[arg(long = "package-id")]
        package_id: Option<String>,
        #[arg(long = "version")]
        version: Option<String>,
        #[arg(long = "configuration")]
        configuration: Option<String>,
        #[arg(long = "path")]
        path: Option<PathBuf>,
        #[arg(long = "auto-build")]
        auto_build: bool,
    },
    List {
        #[arg(long = "path")]
        path: Option<PathBuf>,
    },
    Build {
        name: String,
        #[arg(long = "path")]
        path: Option<PathBuf>,
        #[arg(long = "force")]
        force: bool,
    },
    BuildAll {
        #[arg(long = "path")]
        path: Option<PathBuf>,
        #[arg(long = "force")]
        force: bool,
    },
    Update {
        name: String,
        #[arg(long = "path")]
        path: Option<PathBuf>,
    },
    Sync {
        #[arg(long = "path")]
        path: Option<PathBuf>,
        #[arg(long = "force")]
        force: bool,
    },
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, Commands, NugetAction, NugetCacheAction, NugetFeedAction, NugetSourceAction};

    #[test]
    fn parses_nuget_source_add_command() {
        let cli = Cli::try_parse_from([
            "mntpack",
            "nuget",
            "source",
            "add",
            "CS2Luau.Roblox",
            "--repo",
            "owner/repo",
            "--project",
            "src/CS2Luau.Roblox/CS2Luau.Roblox.csproj",
            "--package-id",
            "CS2Luau.Roblox",
        ])
        .expect("parse cli");

        match cli.command {
            Commands::Nuget {
                action:
                    NugetAction::Source {
                        action: NugetSourceAction::Add { name, repo, .. },
                    },
            } => {
                assert_eq!(name, "CS2Luau.Roblox");
                assert_eq!(repo, "owner/repo");
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_nuget_feed_list_command() {
        let cli = Cli::try_parse_from(["mntpack", "nuget", "feed", "list"]).expect("parse cli");

        match cli.command {
            Commands::Nuget {
                action:
                    NugetAction::Feed {
                        action: NugetFeedAction::List,
                    },
            } => {}
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_nuget_use_command() {
        let cli =
            Cli::try_parse_from(["mntpack", "nuget", "use", "CS2Luau.Roblox"]).expect("parse cli");

        match cli.command {
            Commands::Nuget {
                action: NugetAction::Use { package, .. },
            } => assert_eq!(package, "CS2Luau.Roblox"),
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_nuget_cache_clear_command() {
        let cli = Cli::try_parse_from([
            "mntpack",
            "nuget",
            "cache",
            "clear",
            "CS2Luau.Compiler",
            "1.0.0-local.2",
        ])
        .expect("parse cli");

        match cli.command {
            Commands::Nuget {
                action:
                    NugetAction::Cache {
                        action: NugetCacheAction::Clear { package, version },
                    },
            } => {
                assert_eq!(package, "CS2Luau.Compiler");
                assert_eq!(version.as_deref(), Some("1.0.0-local.2"));
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }
}
