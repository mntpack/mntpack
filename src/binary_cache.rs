use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};

use crate::{
    config::RuntimeContext,
    github::clone::sync_repo,
    package::{
        resolver::resolve_repo,
        store::{first_file_in_dir, normalize_hash, sha256_file},
    },
};

pub fn enabled(runtime: &RuntimeContext) -> bool {
    runtime.config.binary_cache.enabled
        && runtime
            .config
            .binary_cache
            .repo
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
}

pub fn try_download_cached_binary(
    runtime: &RuntimeContext,
    package_repo_name: &str,
    hash: &str,
) -> Result<Option<PathBuf>> {
    let Some(cache_checkout) = ensure_cache_checkout(runtime)? else {
        return Ok(None);
    };
    let hash = normalize_hash(hash);
    let package_dir = cache_checkout.join(package_repo_name).join(&hash);
    if !package_dir.exists() {
        return Ok(None);
    }
    let Some(source) = first_file_in_dir(&package_dir) else {
        return Ok(None);
    };

    let actual = sha256_file(&source)?;
    if normalize_hash(&actual) != hash {
        bail!(
            "binary cache hash mismatch for {}: expected {}, got {}",
            source.display(),
            hash,
            actual
        );
    }

    let out_dir = runtime.paths.cache.join("binary-cache-download");
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;
    let file_name = source
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("binary");
    let destination = out_dir.join(file_name);
    fs::copy(&source, &destination).with_context(|| {
        format!(
            "failed to copy cached binary {} -> {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(Some(destination))
}

pub fn upload_binary_to_cache(
    runtime: &RuntimeContext,
    package_repo_name: &str,
    hash: &str,
    binary_path: &Path,
) -> Result<()> {
    let Some(cache_checkout) = ensure_cache_checkout(runtime)? else {
        bail!("binary cache is not configured");
    };
    let hash = normalize_hash(hash);
    let target_dir = cache_checkout.join(package_repo_name).join(&hash);
    fs::create_dir_all(&target_dir)
        .with_context(|| format!("failed to create {}", target_dir.display()))?;

    let file_name = binary_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("binary");
    let target_file = target_dir.join(file_name);
    if !target_file.exists() {
        fs::copy(binary_path, &target_file).with_context(|| {
            format!(
                "failed to copy {} -> {}",
                binary_path.display(),
                target_file.display()
            )
        })?;
    }

    run_git(
        runtime,
        &cache_checkout,
        &["add", "."],
        "failed to stage binary cache updates",
    )?;

    let diff_status = Command::new(&runtime.config.paths.git)
        .arg("-C")
        .arg(&cache_checkout)
        .args(["diff", "--cached", "--quiet"])
        .status()
        .with_context(|| {
            format!(
                "failed to check staged changes in binary cache {}",
                cache_checkout.display()
            )
        })?;
    if diff_status.success() {
        return Ok(());
    }

    run_git(
        runtime,
        &cache_checkout,
        &[
            "commit",
            "-m",
            &format!("mntpack prebuild {} {}", package_repo_name, hash),
        ],
        "failed to commit binary cache update",
    )?;
    run_git(
        runtime,
        &cache_checkout,
        &["push", "origin", "HEAD"],
        "failed to push binary cache update",
    )?;

    Ok(())
}

fn ensure_cache_checkout(runtime: &RuntimeContext) -> Result<Option<PathBuf>> {
    if !enabled(runtime) {
        return Ok(None);
    }
    let repo_spec = runtime
        .config
        .binary_cache
        .repo
        .as_deref()
        .unwrap_or_default()
        .trim();
    if repo_spec.is_empty() {
        return Ok(None);
    }

    let resolved = resolve_repo(repo_spec, &runtime.config.default_owner)?;
    let checkout = runtime
        .paths
        .cache
        .join("binary-cache")
        .join(&resolved.owner)
        .join(&resolved.repo);
    sync_repo(
        &resolved,
        &checkout,
        &runtime.paths.cache_git,
        &runtime.config.paths.git,
        None,
    )?;
    Ok(Some(checkout))
}

fn run_git(runtime: &RuntimeContext, checkout: &Path, args: &[&str], context: &str) -> Result<()> {
    let status = Command::new(&runtime.config.paths.git)
        .arg("-C")
        .arg(checkout)
        .args(args)
        .status()
        .with_context(|| format!("{context} in {}", checkout.display()))?;
    if !status.success() {
        bail!(
            "{context}: git {} exited with status {:?}",
            args.join(" "),
            status.code()
        );
    }
    Ok(())
}
