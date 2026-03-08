use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "mntpack", version, about = "Mintiler Package Manager")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[command(alias = "add")]
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
    Run {
        package: String,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    List,
    Update {
        package: Option<String>,
    },
    Doctor,
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    Show,
    Get { key: String },
    Set { key: String, value: String },
    Reset,
}
