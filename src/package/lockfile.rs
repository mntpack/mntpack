use std::{collections::BTreeMap, fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
    config::RuntimeContext,
    package::{
        record::{PackageRecord, load_all_records},
        store::prefixed_hash,
    },
};

pub const LOCKFILE_NAME: &str = "mntpack.lock";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LockFile {
    #[serde(default)]
    pub packages: BTreeMap<String, LockPackageEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockPackageEntry {
    pub commit: String,
    pub binary_hash: String,
}

pub fn load_from_cwd() -> Result<Option<LockFile>> {
    let path = lockfile_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let raw =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let lock = serde_json::from_str::<LockFile>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Some(lock))
}

pub fn save_to_cwd(lock: &LockFile) -> Result<()> {
    let path = lockfile_path()?;
    let raw = serde_json::to_string_pretty(lock)?;
    fs::write(&path, raw).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn lockfile_path() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    Ok(cwd.join(LOCKFILE_NAME))
}

pub fn update_entry_for_record(lock: &mut LockFile, record: &PackageRecord) {
    let Some(commit) = record.commit.as_deref() else {
        return;
    };
    let Some(hash) = record.binary_hash.as_deref() else {
        return;
    };
    lock.packages.insert(
        record.repo_spec(),
        LockPackageEntry {
            commit: commit.to_string(),
            binary_hash: prefixed_hash(hash),
        },
    );
}

pub fn regenerate_from_installed(runtime: &RuntimeContext) -> Result<LockFile> {
    let records = load_all_records(&runtime.paths.packages)?;
    let mut lock = LockFile::default();
    for record in records {
        update_entry_for_record(&mut lock, &record);
    }
    Ok(lock)
}
