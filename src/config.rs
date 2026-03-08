use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const APP_DIR: &str = ".mntpack";
const CONFIG_FILE: &str = "config.json";
pub const MNTPACK_HOME_ENV: &str = "MNTPACK_HOME";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct ToolPaths {
    pub git: String,
    pub python: String,
    pub pip: String,
    pub node: String,
    pub npm: String,
    pub cargo: String,
    pub cmake: String,
    pub make: String,
}

impl Default for ToolPaths {
    fn default() -> Self {
        Self {
            git: "git".to_string(),
            python: "python".to_string(),
            pip: "pip".to_string(),
            node: "node".to_string(),
            npm: "npm".to_string(),
            cargo: "cargo".to_string(),
            cmake: "cmake".to_string(),
            make: "make".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default = "default_owner")]
    pub default_owner: String,
    #[serde(default)]
    pub paths: ToolPaths,
    #[serde(default)]
    pub auto_update_on_run: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_owner: default_owner(),
            paths: ToolPaths::default(),
            auto_update_on_run: false,
        }
    }
}

fn default_owner() -> String {
    "MINTILER-DEV".to_string()
}

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root: PathBuf,
    pub config: PathBuf,
    pub repos: PathBuf,
    pub packages: PathBuf,
    pub cache: PathBuf,
    pub cache_git: PathBuf,
    pub cache_exec: PathBuf,
    pub store: PathBuf,
    pub bin: PathBuf,
}

impl AppPaths {
    pub fn package_dir(&self, package_name: &str) -> PathBuf {
        self.packages.join(package_name)
    }

    pub fn repo_dir(&self, repo_key: &str) -> PathBuf {
        self.repos.join(repo_key)
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeContext {
    pub config: Config,
    pub paths: AppPaths,
}

impl RuntimeContext {
    pub fn load_or_init() -> Result<Self> {
        let root = resolve_root_path()?;
        let config_path = root.join(CONFIG_FILE);
        let repos = root.join("repos");
        let packages = root.join("packages");
        let cache = root.join("cache");
        let cache_git = cache.join("git");
        let cache_exec = cache.join("exec");
        let store = root.join("store");
        let bin = root.join("bin");

        for dir in [
            &root,
            &repos,
            &packages,
            &cache,
            &cache_git,
            &cache_exec,
            &store,
            &bin,
        ] {
            fs::create_dir_all(dir)
                .with_context(|| format!("failed to create directory {}", dir.display()))?;
        }

        let config = if config_path.exists() {
            let content = fs::read_to_string(&config_path)
                .with_context(|| format!("failed to read {}", config_path.display()))?;
            serde_json::from_str::<Config>(&content)
                .with_context(|| format!("failed to parse {}", config_path.display()))?
        } else {
            let cfg = Config::default();
            let serialized = serde_json::to_string_pretty(&cfg)?;
            fs::write(&config_path, serialized)
                .with_context(|| format!("failed to write {}", config_path.display()))?;
            cfg
        };

        Ok(Self {
            config,
            paths: AppPaths {
                root,
                config: config_path,
                repos,
                packages,
                cache,
                cache_git,
                cache_exec,
                store,
                bin,
            },
        })
    }

    pub fn save_config(&self, config: &Config) -> Result<()> {
        let serialized = serde_json::to_string_pretty(config)?;
        fs::write(&self.paths.config, serialized)
            .with_context(|| format!("failed to write {}", self.paths.config.display()))?;
        Ok(())
    }
}

fn resolve_root_path() -> Result<PathBuf> {
    if let Ok(custom_home) = std::env::var(MNTPACK_HOME_ENV) {
        let trimmed = custom_home.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    let home = dirs::home_dir().context("unable to locate user home directory")?;
    Ok(home.join(APP_DIR))
}

pub fn repo_key(owner: &str, repo: &str) -> String {
    format!("{owner}__{repo}")
}

pub fn normalize_repo_url(url: &str) -> String {
    if url.ends_with(".git") {
        url.to_string()
    } else {
        format!("{url}.git")
    }
}
