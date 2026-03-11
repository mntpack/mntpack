use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::{
    config::RuntimeContext,
    package::{
        lockfile,
        record::{PackageRecord, find_record_by_package_name, load_all_records},
        resolver::resolve_repo,
        store::sanitize_store_component,
    },
};

const SPECIAL_PACKAGE_NAME: &str = "mntpack";
const SPECIAL_OWNER: &str = "MINTILER-DEV";
const SPECIAL_REPO: &str = "mntpack";

pub fn execute(runtime: &RuntimeContext, input: &str) -> Result<()> {
    let all_records = load_all_records(&runtime.paths.packages)?;
    if all_records.is_empty() {
        bail!("no packages installed");
    }

    let targets = resolve_targets(runtime, input, &all_records)?;
    if targets.is_empty() {
        bail!("package or repository '{input}' is not installed");
    }
    if targets.iter().any(is_protected_mntpack) {
        bail!(
            "package '{}' is protected and cannot be removed",
            SPECIAL_PACKAGE_NAME
        );
    }

    for record in &targets {
        remove_installed_package(runtime, record)?;
    }
    cleanup_repo_directories(runtime, &targets)?;
    refresh_lockfile_if_present(runtime)?;

    let mut names: Vec<String> = targets
        .iter()
        .map(|record| record.package_name.clone())
        .collect();
    names.sort();
    println!("removed {} package(s): {}", targets.len(), names.join(", "));
    Ok(())
}

fn refresh_lockfile_if_present(runtime: &RuntimeContext) -> Result<()> {
    if lockfile::load_from_cwd()?.is_none() {
        return Ok(());
    }
    let lock = lockfile::regenerate_from_installed(runtime)?;
    lockfile::save_to_cwd(&lock)?;
    Ok(())
}

fn resolve_targets(
    runtime: &RuntimeContext,
    input: &str,
    all_records: &[PackageRecord],
) -> Result<Vec<PackageRecord>> {
    if let Some(record) = find_record_by_package_name(&runtime.paths.packages, input)? {
        return Ok(vec![record]);
    }

    let resolved = resolve_repo(input, &runtime.config.default_owner)?;
    Ok(all_records
        .iter()
        .filter(|record| record.owner == resolved.owner && record.repo == resolved.repo)
        .cloned()
        .collect())
}

fn remove_installed_package(runtime: &RuntimeContext, record: &PackageRecord) -> Result<()> {
    if record.global {
        remove_global_shims(runtime, record)?;
    }

    let package_dir = runtime.paths.package_dir(&record.package_name);
    if package_dir.exists() {
        fs::remove_dir_all(&package_dir)
            .with_context(|| format!("failed to remove {}", package_dir.display()))?;
    }

    Ok(())
}

fn remove_global_shims(runtime: &RuntimeContext, record: &PackageRecord) -> Result<()> {
    let mut removed_paths = HashSet::<PathBuf>::new();
    let mut shim_names = vec![record.package_name.clone()];
    if let Some(shim_name) = record.shim_name.as_deref() {
        shim_names.push(shim_name.to_string());
    }

    if let Some(binary_rel_path) = record.binary_rel_path.as_deref() {
        if let Some(stem) = Path::new(binary_rel_path)
            .file_stem()
            .and_then(|value| value.to_str())
            .filter(|value| !value.trim().is_empty())
        {
            shim_names.push(stem.to_string());
        }
    }
    if let Some(binary_path) = record.binary_path.as_deref() {
        if let Some(stem) = Path::new(binary_path)
            .file_stem()
            .and_then(|value| value.to_str())
            .filter(|value| !value.trim().is_empty())
        {
            shim_names.push(stem.to_string());
        }
    }

    shim_names.sort();
    shim_names.dedup();
    for shim_name in shim_names {
        let shim_path = shim_path_for(runtime, &shim_name);
        remove_file_if_exists(&shim_path, &mut removed_paths)?;
    }

    if runtime.paths.bin.exists() {
        let marker = format!("run \"{}\"", record.package_name);
        for entry in fs::read_dir(&runtime.paths.bin)
            .with_context(|| format!("failed to read {}", runtime.paths.bin.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if removed_paths.contains(&path) || !entry.file_type()?.is_file() {
                continue;
            }
            if cfg!(windows) && path.extension().and_then(|ext| ext.to_str()) != Some("cmd") {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            if content.contains(&marker) {
                remove_file_if_exists(&path, &mut removed_paths)?;
            }
        }
    }

    Ok(())
}

fn shim_path_for(runtime: &RuntimeContext, shim_name: &str) -> PathBuf {
    if cfg!(windows) {
        runtime.paths.bin.join(format!("{shim_name}.cmd"))
    } else {
        runtime.paths.bin.join(shim_name)
    }
}

fn remove_file_if_exists(path: &Path, removed_paths: &mut HashSet<PathBuf>) -> Result<()> {
    if path.exists() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
        removed_paths.insert(path.to_path_buf());
    }
    Ok(())
}

fn cleanup_repo_directories(runtime: &RuntimeContext, removed: &[PackageRecord]) -> Result<()> {
    let remaining = load_all_records(&runtime.paths.packages)?;
    let mut repo_keys = HashSet::new();
    for record in removed {
        repo_keys.insert(crate::config::repo_key(&record.owner, &record.repo));
    }

    for record in removed {
        let repo_still_used = remaining
            .iter()
            .any(|other| other.owner == record.owner && other.repo == record.repo);
        if !repo_still_used {
            let repo_key = crate::config::repo_key(&record.owner, &record.repo);
            if !repo_keys.contains(&repo_key) {
                continue;
            }
            let repo_dir = runtime
                .paths
                .repo_dir_from_parts(&record.owner, &record.repo);
            if repo_dir.exists() {
                fs::remove_dir_all(&repo_dir)
                    .with_context(|| format!("failed to remove {}", repo_dir.display()))?;
            } else {
                let legacy = runtime
                    .paths
                    .legacy_repo_dir_from_parts(&record.owner, &record.repo);
                if legacy.exists() {
                    fs::remove_dir_all(&legacy)
                        .with_context(|| format!("failed to remove {}", legacy.display()))?;
                }
            }
            repo_keys.remove(&repo_key);

            let owner_dir = runtime.paths.repos.join(&record.owner);
            if owner_dir.exists() {
                let is_empty = fs::read_dir(&owner_dir)
                    .with_context(|| format!("failed to read {}", owner_dir.display()))?
                    .next()
                    .is_none();
                if is_empty {
                    fs::remove_dir_all(&owner_dir)
                        .with_context(|| format!("failed to remove {}", owner_dir.display()))?;
                }
            }

            let versions_dir = runtime
                .paths
                .store
                .join("versions")
                .join(sanitize_store_component(&record.repo));
            if versions_dir.exists() {
                fs::remove_dir_all(&versions_dir)
                    .with_context(|| format!("failed to remove {}", versions_dir.display()))?;
            }
        }
    }

    let remaining_store_entries: HashSet<String> = remaining
        .iter()
        .filter_map(|record| record.store_entry.clone())
        .collect();
    for record in removed {
        let Some(store_entry) = record.store_entry.as_deref() else {
            continue;
        };
        if remaining_store_entries.contains(store_entry) {
            continue;
        }
        let store_path = runtime.paths.store.join(store_entry);
        if store_path.exists() {
            fs::remove_dir_all(&store_path)
                .with_context(|| format!("failed to remove {}", store_path.display()))?;
        }
    }

    Ok(())
}

fn is_protected_mntpack(record: &PackageRecord) -> bool {
    record
        .package_name
        .eq_ignore_ascii_case(SPECIAL_PACKAGE_NAME)
        && record.owner.eq_ignore_ascii_case(SPECIAL_OWNER)
        && record.repo.eq_ignore_ascii_case(SPECIAL_REPO)
}
