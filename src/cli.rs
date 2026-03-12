use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "mntpack", version, about = "MINTILER-DEV Package Manager")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Build {
        recipe: Option<String>,
        #[arg(short = 'G', long = "generate")]
        generate: bool,
    },
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
    Lock {
        #[command(subcommand)]
        action: LockAction,
    },
    Doctor {
        #[arg(short = 'f', long = "fix")]
        fix: bool,
    },
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

#[derive(Debug, Subcommand)]
pub enum LockAction {
    Regenerate,
}
