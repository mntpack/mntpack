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
    Add {
        package: String,
        version: Option<String>,
        #[arg(long = "source")]
        source: Option<String>,
        #[arg(long = "path")]
        path: Option<PathBuf>,
        #[arg(long = "project")]
        project: Option<PathBuf>,
        #[arg(long = "build")]
        build: bool,
    },
    Remove {
        package: String,
        #[arg(long = "path")]
        path: Option<PathBuf>,
        #[arg(long = "project")]
        project: Option<PathBuf>,
        #[arg(long = "build")]
        build: bool,
    },
    List {
        #[arg(long = "path")]
        path: Option<PathBuf>,
    },
    Install {
        #[arg(long = "path")]
        path: Option<PathBuf>,
        #[arg(long = "project")]
        project: Option<PathBuf>,
        #[arg(long = "build")]
        build: bool,
    },
    Apply {
        #[arg(long = "path")]
        path: Option<PathBuf>,
        #[arg(long = "project")]
        project: Option<PathBuf>,
        #[arg(long = "build")]
        build: bool,
    },
    Restore {
        #[arg(long = "path")]
        path: Option<PathBuf>,
        #[arg(long = "project")]
        project: Option<PathBuf>,
        #[arg(long = "build")]
        build: bool,
    },
    Ensure {
        #[arg(long = "path")]
        path: Option<PathBuf>,
        #[arg(long = "project")]
        project: Option<PathBuf>,
    },
}
