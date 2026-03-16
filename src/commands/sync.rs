use std::{
    collections::HashSet,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use async_recursion::async_recursion;
use git2::Repository;
use tokio::task::JoinSet;
use walkdir::WalkDir;

use crate::{
    binary_cache,
    config::RuntimeContext,
    github::{
        clone::{head_commit_short, sync_repo},
        release::try_download_release_binary,
    },
    installer::{
        driver::{DriverRuntime, InstallContext, run_shell_command},
        manager::{InstallerManager, materialize_binary},
    },
    package::{
        manifest::Manifest,
        record::{
            PackageRecord, find_record_by_package_name, find_record_by_repo, load_record,
            save_record,
        },
        resolver::resolve_repo,
        store::{
            executable_in_hash_store, hash_store_dir, hash_store_entry, require_binary_name,
            sha256_file, version_store_dir,
        },
    },
    shim::generator::{create_shim, ensure_bin_on_path},
    ui::progress::ProgressBar,
};

const PAYLOAD_LINK_NAME: &str = "payload";
const SPECIAL_PACKAGE_NAME: &str = "mntpack";
const SPECIAL_OWNER: &str = "mntpack";
const SPECIAL_REPO: &str = "mntpack";

pub async fn execute(
    runtime: &RuntimeContext,
    repo_input: &str,
    version: Option<&str>,
    release_asset: Option<&str>,
    custom_name: Option<&str>,
    global: bool,
) -> Result<()> {
    validate_release_version_constraints(version, release_asset)?;
    let mut progress = ProgressBar::new("sync", 2);

    let mut effective_repo_input = repo_input.to_string();
    let mut effective_name = custom_name.map(ToString::to_string);

    if is_simple_identifier(repo_input) {
        if let Some(record) = find_record_by_package_name(&runtime.paths.packages, repo_input)? {
            effective_repo_input = record.repo_spec();
            if effective_name.is_none() {
                effective_name = Some(record.package_name);
            }
        }
    }

    let mut visited = HashSet::new();
    let mut record = sync_package_internal(
        runtime,
        &effective_repo_input,
        version,
        release_asset,
        effective_name.as_deref(),
        global,
        &mut visited,
    )
    .await?;
    progress.advance(format!("synced {}", record.package_name));
    if record.binary_hash.is_none() && record.run_command.is_none() {
        record = ensure_package_ready(runtime, &record.package_name).await?;
    }
    progress.advance("prepared package");
    if let Err(err) = crate::sync_dispatch::dispatch_sync(runtime, &record, version).await {
        eprintln!("warning: failed to trigger sync dispatch workflow: {err}");
    }
    progress.finish("ready");
    println!("synced {} ({})", record.package_name, record.repo_spec());
    Ok(())
}

#[async_recursion]
pub async fn sync_package_internal(
    runtime: &RuntimeContext,
    repo_input: &str,
    version: Option<&str>,
    release_asset: Option<&str>,
    custom_name: Option<&str>,
    global: bool,
    visited: &mut HashSet<String>,
) -> Result<PackageRecord> {
    let resolved = resolve_repo(repo_input, &runtime.config.default_owner)?;
    let effective_version = version;

    if visited.contains(&resolved.key) {
        if let Some(record) =
            find_record_by_repo(&runtime.paths.packages, &resolved.owner, &resolved.repo)?
        {
            return Ok(record);
        }
    } else {
        visited.insert(resolved.key.clone());
    }

    let package_name = resolve_package_name(runtime, &resolved.owner, &resolved.repo, custom_name)?;
    let repo_dir = runtime
        .paths
        .repo_dir_from_parts(&resolved.owner, &resolved.repo);
    migrate_legacy_repo_layout(runtime, &resolved.owner, &resolved.repo, &repo_dir)?;
    let package_dir = runtime.paths.package_dir(&package_name);
    let effective_global = global || is_special_repo(&resolved.owner, &resolved.repo);

    sync_repo(
        &resolved,
        &repo_dir,
        &runtime.paths.cache_git,
        &runtime.config.paths.git,
        effective_version,
    )?;
    validate_tag_when_release_selected(&repo_dir, effective_version, release_asset)?;
    let commit = head_commit_short(&repo_dir).ok();
    let manifest = Manifest::load(&repo_dir)?;
    let bin_command = manifest.as_ref().and_then(|m| m.resolve_bin_command());
    let run_command = manifest
        .as_ref()
        .and_then(|m| m.resolve_run_command())
        .or_else(|| bin_command.as_ref().map(|(_, command)| command.clone()));
    let preferred_shim_name = bin_command.as_ref().map(|(name, _)| name.clone());

    if let Some(manifest) = &manifest {
        sync_dependencies_parallel(runtime, manifest.dependencies.clone()).await?;
    }

    let mut installed_binary: Option<PathBuf> = None;
    let mut binary_rel_path: Option<String> = None;
    let mut store_entry: Option<String> = None;
    let mut binary_hash: Option<String> = None;
    let mut binary_name: Option<String> = None;
    let mut build_pending = true;
    let release_requested = release_asset.is_some();
    let mut release_installed = false;
    let mut active_hash = None;

    if active_hash.is_none() {
        if let Some(existing) =
            find_record_by_repo(&runtime.paths.packages, &resolved.owner, &resolved.repo)?
        {
            if existing.commit == commit {
                active_hash = existing.binary_hash;
            }
        }
    }

    if run_command.is_none() {
        if let Some(expected_hash) = active_hash.as_deref() {
            if let Some(binary) =
                executable_in_hash_store(&runtime.paths.store, expected_hash, None)?
            {
                fs::create_dir_all(&package_dir)
                    .with_context(|| format!("failed to create {}", package_dir.display()))?;
                let stored = persist_binary_to_store(
                    runtime,
                    &resolved.repo,
                    effective_version.or(manifest.as_ref().and_then(|m| m.version.as_deref())),
                    commit.as_deref(),
                    &package_dir,
                    &package_name,
                    &binary,
                )?;
                installed_binary = Some(stored.binary_path);
                binary_rel_path = Some(stored.binary_rel_path);
                store_entry = Some(stored.store_entry);
                binary_hash = Some(stored.binary_hash);
                binary_name = Some(stored.binary_name);
                build_pending = false;
            } else {
                match binary_cache::try_download_cached_binary(
                    runtime,
                    &resolved.key,
                    expected_hash,
                ) {
                    Ok(Some(cached)) => {
                        fs::create_dir_all(&package_dir).with_context(|| {
                            format!("failed to create {}", package_dir.display())
                        })?;
                        let stored = persist_binary_to_store(
                            runtime,
                            &resolved.repo,
                            effective_version
                                .or(manifest.as_ref().and_then(|m| m.version.as_deref())),
                            commit.as_deref(),
                            &package_dir,
                            &package_name,
                            &cached,
                        )?;
                        installed_binary = Some(stored.binary_path);
                        binary_rel_path = Some(stored.binary_rel_path);
                        store_entry = Some(stored.store_entry);
                        binary_hash = Some(stored.binary_hash);
                        binary_name = Some(stored.binary_name);
                        build_pending = false;
                    }
                    Ok(None) => {}
                    Err(err) => eprintln!(
                        "warning: failed to read binary cache entry for '{}': {}",
                        resolved.key, err
                    ),
                }
            }
        }
    }

    if !release_requested && run_command.is_none() && installed_binary.is_none() {
        match binary_cache::try_download_cached_release_binary(
            runtime,
            &resolved.key,
            effective_version,
            commit.as_deref(),
        )
        .await
        {
            Ok(Some(cached_release)) => {
                fs::create_dir_all(&package_dir)
                    .with_context(|| format!("failed to create {}", package_dir.display()))?;
                let stored = persist_binary_to_store(
                    runtime,
                    &resolved.repo,
                    effective_version.or(manifest.as_ref().and_then(|m| m.version.as_deref())),
                    commit.as_deref(),
                    &package_dir,
                    &package_name,
                    &cached_release,
                )?;
                installed_binary = Some(stored.binary_path);
                binary_rel_path = Some(stored.binary_rel_path);
                store_entry = Some(stored.store_entry);
                binary_hash = Some(stored.binary_hash);
                binary_name = Some(stored.binary_name);
                build_pending = false;
            }
            Ok(None) => {}
            Err(err) => eprintln!(
                "warning: failed to download prebuilt binary from cache for '{}': {}",
                resolved.key, err
            ),
        }
    }

    if run_command.is_none() && installed_binary.is_none() {
        if let Some(release_binary) = try_download_release_binary(
            runtime,
            &resolved,
            manifest.as_ref(),
            effective_version,
            release_asset,
        )
        .await?
        {
            fs::create_dir_all(&package_dir)
                .with_context(|| format!("failed to create {}", package_dir.display()))?;
            let staged = materialize_binary(&release_binary, &package_dir, &package_name)?;
            let stored = persist_binary_to_store(
                runtime,
                &resolved.repo,
                effective_version.or(manifest.as_ref().and_then(|m| m.version.as_deref())),
                commit.as_deref(),
                &package_dir,
                &package_name,
                &staged,
            )?;
            installed_binary = Some(stored.binary_path);
            binary_rel_path = Some(stored.binary_rel_path);
            store_entry = Some(stored.store_entry);
            binary_hash = Some(stored.binary_hash);
            binary_name = Some(stored.binary_name);
            build_pending = false;
            release_installed = true;
        }
    }

    if release_requested && run_command.is_none() && !release_installed {
        bail!(
            "no matching GitHub release asset found for '{}'",
            resolved.key
        );
    }

    if is_special_repo(&resolved.owner, &resolved.repo)
        && run_command.is_none()
        && installed_binary.is_none()
    {
        match build_special_repo_binary(runtime, &repo_dir, &package_name) {
            Ok(binary) => {
                let stored = persist_binary_to_store(
                    runtime,
                    &resolved.repo,
                    effective_version
                        .or(manifest.as_ref().and_then(|m| m.version.as_deref()))
                        .or(commit.as_deref()),
                    commit.as_deref(),
                    &package_dir,
                    &package_name,
                    &binary,
                )?;
                installed_binary = Some(stored.binary_path);
                binary_rel_path = Some(stored.binary_rel_path);
                store_entry = Some(stored.store_entry);
                binary_hash = Some(stored.binary_hash);
                binary_name = Some(stored.binary_name);
                build_pending = false;
            }
            Err(err) => {
                eprintln!(
                    "warning: failed to rebuild managed mntpack from synced repo, falling back to current executable: {err}"
                );
                fs::create_dir_all(&package_dir)
                    .with_context(|| format!("failed to create {}", package_dir.display()))?;
                let staged = stage_current_executable(&package_dir)?;
                let stored = persist_binary_to_store(
                    runtime,
                    &resolved.repo,
                    effective_version
                        .or(manifest.as_ref().and_then(|m| m.version.as_deref()))
                        .or(commit.as_deref()),
                    commit.as_deref(),
                    &package_dir,
                    &package_name,
                    &staged,
                )?;
                installed_binary = Some(stored.binary_path);
                binary_rel_path = Some(stored.binary_rel_path);
                store_entry = Some(stored.store_entry);
                binary_hash = Some(stored.binary_hash);
                binary_name = Some(stored.binary_name);
                build_pending = false;
            }
        }
    }

    let shim_name = preferred_shim_name.unwrap_or_else(|| package_name.clone());
    if effective_global {
        create_shim(
            runtime,
            &package_name,
            &shim_name,
            installed_binary.as_deref(),
        )?;
        if ensure_bin_on_path(runtime)? {
            println!(
                "added '{}' to PATH for global shims",
                runtime.paths.bin.display()
            );
        }
    }

    let record = PackageRecord {
        package_name,
        owner: resolved.owner.clone(),
        repo: resolved.repo.clone(),
        version: effective_version
            .map(|v| v.to_string())
            .or_else(|| manifest.as_ref().and_then(|m| m.version.clone())),
        commit,
        run_command,
        binary_rel_path,
        binary_path: installed_binary.map(|path| path.to_string_lossy().to_string()),
        binary_hash,
        binary_name,
        shim_name: Some(shim_name),
        store_entry,
        build_pending,
        global: effective_global,
    };
    save_record(&package_dir, &record)?;

    Ok(record)
}

pub async fn ensure_package_ready(
    runtime: &RuntimeContext,
    package_name: &str,
) -> Result<PackageRecord> {
    let package_dir = runtime.paths.package_dir(package_name);
    let Some(record) = load_record(&package_dir)? else {
        bail!("package metadata for '{package_name}' is missing");
    };

    if !requires_prepare(runtime, &record) {
        return Ok(record);
    }

    prepare_package(runtime, record).await
}

fn requires_prepare(runtime: &RuntimeContext, record: &PackageRecord) -> bool {
    if record.build_pending {
        return true;
    }
    if record.run_command.is_some() {
        return false;
    }
    resolve_binary_path(runtime, record)
        .map(|path| !path.exists())
        .unwrap_or(true)
}

async fn prepare_package(
    runtime: &RuntimeContext,
    mut record: PackageRecord,
) -> Result<PackageRecord> {
    let repo_dir = runtime
        .paths
        .repo_dir_existing_or_new(&record.owner, &record.repo);
    if !repo_dir.exists() {
        bail!("repository directory not found at {}", repo_dir.display());
    }
    record.commit = head_commit_short(&repo_dir).ok();

    let package_dir = runtime.paths.package_dir(&record.package_name);
    fs::create_dir_all(&package_dir)
        .with_context(|| format!("failed to create {}", package_dir.display()))?;

    let manifest = Manifest::load(&repo_dir)?;
    let bin_command = manifest.as_ref().and_then(|m| m.resolve_bin_command());
    let run_command = manifest
        .as_ref()
        .and_then(|m| m.resolve_run_command())
        .or_else(|| bin_command.as_ref().map(|(_, command)| command.clone()))
        .or_else(|| record.run_command.clone());
    let mut shim_name = bin_command
        .as_ref()
        .map(|(name, _)| name.clone())
        .or_else(|| record.shim_name.clone())
        .unwrap_or_else(|| record.package_name.clone());

    if let Some(script) = manifest.as_ref().and_then(|m| m.preinstall.as_deref()) {
        run_script(script, &repo_dir)?;
    }

    let runtime_driver = DriverRuntime { runtime };
    let installer_ctx = InstallContext {
        package_name: record.package_name.clone(),
        repo_path: repo_dir.clone(),
        package_dir: package_dir.clone(),
        manifest: manifest.clone(),
    };

    let mut installed_binary = None;
    if run_command.is_none() {
        if let Some(manifest) = &manifest {
            if let Some(release_binary) = try_download_release_binary(
                runtime,
                &resolve_repo(&record.repo_spec(), &runtime.config.default_owner)?,
                Some(manifest),
                record.version.as_deref(),
                None,
            )
            .await?
            {
                installed_binary = Some(materialize_binary(
                    &release_binary,
                    &package_dir,
                    &record.package_name,
                )?);
            }
        }
    }

    if installed_binary.is_none() || run_command.is_some() {
        let result = InstallerManager::new().install(&installer_ctx, &runtime_driver)?;
        if let Some(binary) = result.binary_path {
            installed_binary = Some(binary);
        }
        shim_name = result.shim_name;
    }

    if let Some(script) = manifest.as_ref().and_then(|m| m.postinstall.as_deref()) {
        run_script(script, &repo_dir)?;
    }

    let mut binary_rel_path = None;
    let mut store_entry = None;
    let mut binary_path = None;
    let mut binary_hash = None;
    let mut binary_name = None;
    if let Some(binary) = installed_binary {
        let stored = persist_binary_to_store(
            runtime,
            &record.repo,
            record.version.as_deref(),
            record.commit.as_deref(),
            &package_dir,
            &record.package_name,
            &binary,
        )?;
        binary_rel_path = Some(stored.binary_rel_path);
        binary_path = Some(stored.binary_path.to_string_lossy().to_string());
        store_entry = Some(stored.store_entry);
        binary_hash = Some(stored.binary_hash);
        binary_name = Some(stored.binary_name);
    }

    record.run_command = run_command;
    record.shim_name = Some(shim_name.clone());
    record.binary_rel_path = binary_rel_path;
    record.binary_path = binary_path;
    record.binary_hash = binary_hash;
    record.binary_name = binary_name;
    record.store_entry = store_entry;
    record.build_pending = false;

    save_record(&package_dir, &record)?;
    if record.global {
        let binary = resolve_binary_path(runtime, &record);
        create_shim(runtime, &record.package_name, &shim_name, binary.as_deref())?;
    }
    Ok(record)
}

pub fn resolve_binary_path(runtime: &RuntimeContext, record: &PackageRecord) -> Option<PathBuf> {
    if let (Some(hash), Some(name)) = (record.binary_hash.as_deref(), record.binary_name.as_deref())
    {
        let path = hash_store_dir(&runtime.paths.store, hash).join(name);
        if path.exists() {
            return Some(path);
        }
    }

    if let Some(explicit) = record.binary_path.as_deref() {
        let path = PathBuf::from(explicit);
        if path.is_absolute() {
            return Some(path);
        }
        return Some(runtime.paths.root.join(path));
    }

    record.binary_rel_path.as_ref().map(|relative| {
        runtime
            .paths
            .package_dir(&record.package_name)
            .join(relative)
    })
}

struct StorePlacement {
    binary_path: PathBuf,
    binary_rel_path: String,
    store_entry: String,
    binary_hash: String,
    binary_name: String,
}

fn persist_binary_to_store(
    runtime: &RuntimeContext,
    repo_name: &str,
    version: Option<&str>,
    commit: Option<&str>,
    package_dir: &Path,
    package_name: &str,
    source_binary: &Path,
) -> Result<StorePlacement> {
    fs::create_dir_all(&runtime.paths.store)
        .with_context(|| format!("failed to create {}", runtime.paths.store.display()))?;

    let binary_hash = sha256_file(source_binary)?;
    let store_entry = hash_store_entry(&binary_hash);
    let store_dir = hash_store_dir(&runtime.paths.store, &binary_hash);
    fs::create_dir_all(&store_dir)
        .with_context(|| format!("failed to create {}", store_dir.display()))?;

    let fallback_name = if cfg!(windows) {
        format!("{package_name}.exe")
    } else {
        package_name.to_string()
    };
    let file_name = require_binary_name(source_binary, &fallback_name)?;
    let stored_binary = store_dir.join(&file_name);
    if !stored_binary.exists() {
        copy_file_with_retry(source_binary, &stored_binary)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&stored_binary)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&stored_binary, perms)?;
        }
    }
    create_version_store_aliases(
        runtime,
        repo_name,
        version,
        commit,
        &stored_binary,
        &file_name,
    )?;

    if package_name.eq_ignore_ascii_case(SPECIAL_PACKAGE_NAME) {
        let payload_dir = package_dir.join(PAYLOAD_LINK_NAME);
        let running_from_payload = std::env::current_exe()
            .ok()
            .map(|exe| exe.starts_with(&payload_dir))
            .unwrap_or(false);
        if !running_from_payload {
            if let Err(err) = link_store_payload(package_dir, &store_dir) {
                eprintln!("warning: unable to relink mntpack payload directory: {err}");
            }
        }

        let rel = if payload_dir.exists() {
            format!("{PAYLOAD_LINK_NAME}/{file_name}")
        } else {
            stored_binary
                .strip_prefix(&runtime.paths.root)
                .unwrap_or(&stored_binary)
                .to_string_lossy()
                .replace('\\', "/")
        };
        if source_binary.starts_with(package_dir) && source_binary != stored_binary {
            let _ = fs::remove_file(source_binary);
        }
        return Ok(StorePlacement {
            binary_path: stored_binary,
            binary_rel_path: rel,
            store_entry,
            binary_hash,
            binary_name: file_name,
        });
    }

    link_store_payload(package_dir, &store_dir)?;
    if source_binary.starts_with(package_dir) && source_binary != stored_binary {
        let _ = fs::remove_file(source_binary);
    }

    Ok(StorePlacement {
        binary_path: stored_binary,
        binary_rel_path: format!("{PAYLOAD_LINK_NAME}/{file_name}"),
        store_entry,
        binary_hash,
        binary_name: file_name,
    })
}

fn link_store_payload(package_dir: &Path, store_dir: &Path) -> Result<()> {
    fs::create_dir_all(package_dir)
        .with_context(|| format!("failed to create {}", package_dir.display()))?;
    let payload_link = package_dir.join(PAYLOAD_LINK_NAME);
    if fs::symlink_metadata(&payload_link).is_ok() {
        remove_path(&payload_link)?;
    }

    if try_symlink_dir(store_dir, &payload_link).is_err() {
        fs::create_dir_all(&payload_link)
            .with_context(|| format!("failed to create {}", payload_link.display()))?;
        for entry in WalkDir::new(store_dir).min_depth(1).into_iter().flatten() {
            let rel = match entry.path().strip_prefix(store_dir) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let target = payload_link.join(rel);
            if entry.file_type().is_dir() {
                fs::create_dir_all(&target)
                    .with_context(|| format!("failed to create {}", target.display()))?;
                continue;
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            fs::copy(entry.path(), &target).with_context(|| {
                format!(
                    "failed to copy {} -> {}",
                    entry.path().display(),
                    target.display()
                )
            })?;
        }
    }

    Ok(())
}

fn try_symlink_dir(target: &Path, link: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link).with_context(|| {
            format!(
                "failed to symlink {} -> {}",
                link.display(),
                target.display()
            )
        })
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).with_context(|| {
            format!(
                "failed to symlink {} -> {}",
                link.display(),
                target.display()
            )
        })
    }
}

fn remove_path(path: &Path) -> Result<()> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    } else if metadata.is_dir() {
        fs::remove_dir_all(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}

fn create_version_store_aliases(
    runtime: &RuntimeContext,
    repo_name: &str,
    version: Option<&str>,
    commit: Option<&str>,
    stored_binary: &Path,
    binary_name: &str,
) -> Result<()> {
    let labels = version_alias_labels(version, commit);
    for label in labels {
        let alias_dir = version_store_dir(&runtime.paths.store, repo_name, &label);
        fs::create_dir_all(&alias_dir)
            .with_context(|| format!("failed to create {}", alias_dir.display()))?;
        let alias_binary = alias_dir.join(binary_name);
        if alias_binary.exists() {
            continue;
        }
        if try_symlink_file(stored_binary, &alias_binary).is_err() {
            copy_file_with_retry(stored_binary, &alias_binary)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&alias_binary)?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&alias_binary, perms)?;
            }
        }
    }
    Ok(())
}

fn version_alias_labels(version: Option<&str>, commit: Option<&str>) -> Vec<String> {
    let mut labels = Vec::new();
    if let Some(version) = version.map(str::trim).filter(|value| !value.is_empty()) {
        labels.push(version.to_string());
    }
    if let Some(commit) = commit.map(str::trim).filter(|value| !value.is_empty()) {
        labels.push(commit.to_string());
    }
    labels.sort();
    labels.dedup();
    labels
}

fn try_symlink_file(target: &Path, link: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(target, link).with_context(|| {
            format!(
                "failed to symlink {} -> {}",
                link.display(),
                target.display()
            )
        })
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).with_context(|| {
            format!(
                "failed to symlink {} -> {}",
                link.display(),
                target.display()
            )
        })
    }
}

fn copy_file_with_retry(source: &Path, destination: &Path) -> Result<()> {
    let mut last_error: Option<std::io::Error> = None;
    for attempt in 0..60 {
        match fs::copy(source, destination) {
            Ok(_) => return Ok(()),
            Err(err) => {
                let locked = cfg!(windows) && err.raw_os_error() == Some(32);
                if locked && attempt < 59 {
                    last_error = Some(err);
                    thread::sleep(Duration::from_millis(500));
                    continue;
                }
                return Err(err).with_context(|| {
                    format!(
                        "failed to copy binary {} -> {}",
                        source.display(),
                        destination.display()
                    )
                });
            }
        }
    }

    let err =
        last_error.unwrap_or_else(|| std::io::Error::other("copy failed after retry attempts"));
    Err(err).with_context(|| {
        format!(
            "failed to copy binary {} -> {}",
            source.display(),
            destination.display()
        )
    })
}

fn stage_current_executable(package_dir: &Path) -> Result<PathBuf> {
    let current_exe = std::env::current_exe().context("failed to resolve current executable")?;
    if !current_exe.exists() {
        bail!("current executable not found at {}", current_exe.display());
    }
    let file_name = if cfg!(windows) {
        "mntpack.exe".to_string()
    } else {
        "mntpack".to_string()
    };
    let destination = package_dir.join(file_name);
    fs::copy(&current_exe, &destination).with_context(|| {
        format!(
            "failed to stage current executable {} -> {}",
            current_exe.display(),
            destination.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&destination)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&destination, perms)?;
    }

    Ok(destination)
}

fn build_special_repo_binary(
    runtime: &RuntimeContext,
    repo_dir: &Path,
    package_name: &str,
) -> Result<PathBuf> {
    let target_dir = repo_dir.join(".mntpack-build-target");
    let status = Command::new(&runtime.config.paths.cargo)
        .args(["build", "--release"])
        .env("CARGO_TARGET_DIR", &target_dir)
        .current_dir(repo_dir)
        .status()
        .with_context(|| {
            format!(
                "failed to run '{} build --release' in {}",
                runtime.config.paths.cargo,
                repo_dir.display()
            )
        })?;
    if !status.success() {
        bail!(
            "command '{}' failed with exit code {:?}",
            format!("{} build --release", runtime.config.paths.cargo),
            status.code()
        );
    }

    let release_dir = target_dir.join("release");
    let candidates = if cfg!(windows) {
        vec![
            release_dir.join(format!("{package_name}.exe")),
            release_dir.join(format!("{}.exe", package_name.replace('-', "_"))),
            release_dir.join("mntpack.exe"),
        ]
    } else {
        vec![
            release_dir.join(package_name),
            release_dir.join(package_name.replace('-', "_")),
            release_dir.join("mntpack"),
        ]
    };
    for candidate in candidates {
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    bail!(
        "managed rebuild completed but no binary found in {}",
        release_dir.display()
    )
}

async fn sync_dependencies_parallel(
    runtime: &RuntimeContext,
    dependencies: Vec<String>,
) -> Result<()> {
    if dependencies.is_empty() {
        return Ok(());
    }
    let mut jobs = JoinSet::new();
    for dependency in dependencies {
        let runtime_clone = runtime.clone();
        jobs.spawn(async move {
            let mut dependency_visited = HashSet::new();
            sync_package_internal(
                &runtime_clone,
                &dependency,
                None,
                None,
                None,
                false,
                &mut dependency_visited,
            )
            .await
        });
    }

    while let Some(result) = jobs.join_next().await {
        let record = result.context("dependency install task panicked")??;
        println!(
            "synced dependency {} ({})",
            record.package_name,
            record.repo_spec()
        );
    }
    Ok(())
}

fn run_script(script: &str, repo_dir: &Path) -> Result<()> {
    run_shell_command(script, repo_dir)
}

fn resolve_package_name(
    runtime: &RuntimeContext,
    owner: &str,
    repo: &str,
    custom_name: Option<&str>,
) -> Result<String> {
    let desired = custom_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| repo.to_string());

    if desired.eq_ignore_ascii_case(SPECIAL_PACKAGE_NAME) && !is_special_repo(owner, repo) {
        bail!("package name '{}' is reserved", SPECIAL_PACKAGE_NAME);
    }
    if is_special_repo(owner, repo) && !desired.eq_ignore_ascii_case(SPECIAL_PACKAGE_NAME) {
        bail!(
            "the official mntpack repository must use package name '{}'",
            SPECIAL_PACKAGE_NAME
        );
    }

    if !is_conflicting_name(runtime, &desired, owner, repo)? {
        return Ok(desired);
    }

    if custom_name.is_some() {
        bail!(
            "package name '{}' is already used by another repository. choose a different --name",
            desired
        );
    }

    prompt_for_custom_name(runtime, owner, repo, &desired)
}

fn is_conflicting_name(
    runtime: &RuntimeContext,
    name: &str,
    owner: &str,
    repo: &str,
) -> Result<bool> {
    let package_dir = runtime.paths.package_dir(name);
    if let Some(record) = load_record(&package_dir)? {
        return Ok(!(record.owner == owner && record.repo == repo));
    }

    Ok(false)
}

fn prompt_for_custom_name(
    runtime: &RuntimeContext,
    owner: &str,
    repo: &str,
    conflicting_name: &str,
) -> Result<String> {
    println!(
        "package name '{}' is already used. choose a custom package name:",
        conflicting_name
    );

    loop {
        print!("custom name: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let candidate = input.trim();
        if candidate.is_empty() {
            println!("name cannot be empty");
            continue;
        }
        if is_conflicting_name(runtime, candidate, owner, repo)? {
            println!("'{}' is already in use, choose another", candidate);
            continue;
        }
        return Ok(candidate.to_string());
    }
}

fn is_simple_identifier(input: &str) -> bool {
    !input.contains('/') && !input.contains("://")
}

fn validate_release_version_constraints(
    version: Option<&str>,
    release_asset: Option<&str>,
) -> Result<()> {
    if release_asset.is_none() {
        return Ok(());
    }
    if let Some(version) = version {
        if looks_like_commit_hash(version) {
            bail!("-r/--release cannot be used when -v is a commit hash; -v must be a tag");
        }
    }
    Ok(())
}

fn validate_tag_when_release_selected(
    repo_dir: &Path,
    version: Option<&str>,
    release_asset: Option<&str>,
) -> Result<()> {
    if release_asset.is_none() || version.is_none() {
        return Ok(());
    }
    let version = version.unwrap_or_default();
    if looks_like_commit_hash(version) {
        bail!("-r/--release cannot be used when -v is a commit hash; -v must be a tag");
    }
    let repo = Repository::open(repo_dir)?;
    let tag_ref = format!("refs/tags/{version}");
    if repo.find_reference(&tag_ref).is_err() {
        bail!(
            "-r/--release with -v requires -v to be a tag; '{version}' is not a tag in this repository"
        );
    }
    Ok(())
}

fn looks_like_commit_hash(input: &str) -> bool {
    let len = input.len();
    if !(7..=40).contains(&len) {
        return false;
    }
    input.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_special_repo(owner: &str, repo: &str) -> bool {
    owner.eq_ignore_ascii_case(SPECIAL_OWNER) && repo.eq_ignore_ascii_case(SPECIAL_REPO)
}

fn migrate_legacy_repo_layout(
    runtime: &RuntimeContext,
    owner: &str,
    repo: &str,
    new_repo_dir: &Path,
) -> Result<()> {
    let legacy_repo_dir = runtime.paths.legacy_repo_dir_from_parts(owner, repo);
    if !legacy_repo_dir.exists() || new_repo_dir.exists() {
        return Ok(());
    }

    if let Some(parent) = new_repo_dir.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::rename(&legacy_repo_dir, new_repo_dir).with_context(|| {
        format!(
            "failed to migrate legacy repo path {} -> {}",
            legacy_repo_dir.display(),
            new_repo_dir.display()
        )
    })?;
    Ok(())
}
