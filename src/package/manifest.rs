use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const MANIFEST_FILE: &str = "mntpack.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReleaseAssetConfig {
    pub file: String,
    pub bin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RunConfig {
    Single(String),
    PerTarget(HashMap<String, String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BinConfig {
    Path(String),
    Commands(HashMap<String, String>),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub name: Option<String>,
    pub version: Option<String>,
    pub preinstall: Option<String>,
    pub postinstall: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    pub build: Option<String>,
    pub bin: Option<BinConfig>,
    pub run: Option<RunConfig>,
    #[serde(default)]
    pub release: HashMap<String, ReleaseAssetConfig>,
}

impl Manifest {
    pub fn load(repo_path: &Path) -> Result<Option<Self>> {
        let file = repo_path.join(MANIFEST_FILE);
        if !file.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&file)
            .with_context(|| format!("failed to read {}", file.display()))?;
        let manifest = serde_json::from_str::<Self>(&content)
            .with_context(|| format!("failed to parse {}", file.display()))?;
        Ok(Some(manifest))
    }

    pub fn resolve_run_command(&self) -> Option<String> {
        let run = self.run.as_ref()?;
        match run {
            RunConfig::Single(command) => Some(command.clone()),
            RunConfig::PerTarget(targets) => targets.get(current_target()).cloned(),
        }
    }

    pub fn resolve_bin_path(&self) -> Option<String> {
        match self.bin.as_ref()? {
            BinConfig::Path(path) => Some(path.clone()),
            BinConfig::Commands(_) => None,
        }
    }

    pub fn resolve_bin_command(&self) -> Option<(String, String)> {
        match self.bin.as_ref()? {
            BinConfig::Path(_) => None,
            BinConfig::Commands(commands) => {
                let mut entries: Vec<(&String, &String)> = commands.iter().collect();
                entries.sort_by(|a, b| a.0.cmp(b.0));
                entries
                    .into_iter()
                    .next()
                    .map(|(name, command)| (name.clone(), command.clone()))
            }
        }
    }
}

fn current_target() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "windows-x64",
        ("windows", "x86") => "windows-x86",
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-arm64",
        ("macos", "x86_64") => "macos-x64",
        ("macos", "aarch64") => "macos-arm64",
        _ => "unknown",
    }
}
