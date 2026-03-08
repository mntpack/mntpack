use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const RECORD_FILE: &str = "install.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageRecord {
    pub package_name: String,
    pub owner: String,
    pub repo: String,
    pub version: Option<String>,
    #[serde(default)]
    pub commit: Option<String>,
    pub binary_rel_path: Option<String>,
    #[serde(default)]
    pub binary_path: Option<String>,
    pub run_command: Option<String>,
    #[serde(default)]
    pub shim_name: Option<String>,
    #[serde(default)]
    pub store_entry: Option<String>,
    #[serde(default)]
    pub build_pending: bool,
    pub global: bool,
}

impl PackageRecord {
    pub fn repo_spec(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }
}

pub fn save_record(package_dir: &Path, record: &PackageRecord) -> Result<()> {
    fs::create_dir_all(package_dir)
        .with_context(|| format!("failed to create {}", package_dir.display()))?;
    let payload = serde_json::to_string_pretty(record)?;
    let path = package_dir.join(RECORD_FILE);
    fs::write(&path, payload).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn load_record(package_dir: &Path) -> Result<Option<PackageRecord>> {
    let path = package_dir.join(RECORD_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let record = serde_json::from_str::<PackageRecord>(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Some(record))
}

pub fn load_all_records(packages_root: &Path) -> Result<Vec<PackageRecord>> {
    if !packages_root.exists() {
        return Ok(Vec::new());
    }
    let mut records = Vec::new();
    for entry in fs::read_dir(packages_root)
        .with_context(|| format!("failed to read {}", packages_root.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        if let Some(record) = load_record(&entry.path())? {
            records.push(record);
        }
    }
    records.sort_by(|a, b| a.package_name.cmp(&b.package_name));
    Ok(records)
}

pub fn find_record_by_repo(
    packages_root: &Path,
    owner: &str,
    repo: &str,
) -> Result<Option<PackageRecord>> {
    let records = load_all_records(packages_root)?;
    Ok(records
        .into_iter()
        .find(|record| record.owner == owner && record.repo == repo))
}

pub fn find_record_by_package_name(
    packages_root: &Path,
    package_name: &str,
) -> Result<Option<PackageRecord>> {
    let package_dir = packages_root.join(package_name);
    load_record(&package_dir)
}
