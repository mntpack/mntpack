use std::{
    collections::HashSet,
    io::{self, Write},
    path::Path,
};

use anyhow::{Result, bail};
use async_recursion::async_recursion;
use git2::Repository;

use crate::{
    config::RuntimeContext,
    github::{clone::sync_repo, release::try_download_release_binary},
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
    },
    shim::generator::{create_shim, ensure_bin_on_path},
};

pub async fn execute(
    runtime: &RuntimeContext,
    repo_input: &str,
    version: Option<&str>,
    release_asset: Option<&str>,
    custom_name: Option<&str>,
    global: bool,
) -> Result<()> {
    validate_release_version_constraints(version, release_asset)?;

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
    let record = sync_package_internal(
        runtime,
        &effective_repo_input,
        version,
        release_asset,
        effective_name.as_deref(),
        global,
        &mut visited,
    )
    .await?;
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
    let repo_dir = runtime.paths.repo_dir(&resolved.key);
    let package_dir = runtime.paths.package_dir(&package_name);

    sync_repo(&resolved, &repo_dir, version)?;
    validate_tag_when_release_selected(&repo_dir, version, release_asset)?;
    let manifest = Manifest::load(&repo_dir)?;
    let bin_command = manifest.as_ref().and_then(|m| m.resolve_bin_command());
    let run_command = manifest
        .as_ref()
        .and_then(|m| m.resolve_run_command())
        .or_else(|| bin_command.as_ref().map(|(_, command)| command.clone()));
    let preferred_shim_name = bin_command.as_ref().map(|(name, _)| name.clone());

    if let Some(manifest) = &manifest {
        for dependency in &manifest.dependencies {
            sync_package_internal(runtime, dependency, None, None, None, false, visited).await?;
        }
    }

    if let Some(script) = manifest.as_ref().and_then(|m| m.preinstall.as_deref()) {
        run_script(script, &repo_dir)?;
    }

    let runtime_driver = DriverRuntime { runtime };
    let installer_ctx = InstallContext {
        package_name: package_name.clone(),
        repo_path: repo_dir.clone(),
        package_dir: package_dir.clone(),
        manifest: manifest.clone(),
    };

    let (installed_binary, shim_name) = if let Some(manifest) = &manifest {
        if run_command.is_none() {
            if let Some(release_binary) =
                try_download_release_binary(runtime, &resolved, manifest, version, release_asset)
                    .await?
            {
                (
                    Some(materialize_binary(
                        &release_binary,
                        &package_dir,
                        &package_name,
                    )?),
                    package_name.clone(),
                )
            } else {
                let result = InstallerManager::new().install(&installer_ctx, &runtime_driver)?;
                (result.binary_path, result.shim_name)
            }
        } else {
            let result = InstallerManager::new().install(&installer_ctx, &runtime_driver)?;
            (result.binary_path, result.shim_name)
        }
    } else {
        let result = InstallerManager::new().install(&installer_ctx, &runtime_driver)?;
        (result.binary_path, result.shim_name)
    };
    let shim_name = preferred_shim_name.unwrap_or(shim_name);

    if let Some(script) = manifest.as_ref().and_then(|m| m.postinstall.as_deref()) {
        run_script(script, &repo_dir)?;
    }

    if global {
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

    let binary_rel_path = installed_binary.as_ref().map(|path| {
        path.strip_prefix(&package_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string()
    });

    let record = PackageRecord {
        package_name,
        owner: resolved.owner.clone(),
        repo: resolved.repo.clone(),
        version: version.map(|v| v.to_string()),
        run_command,
        binary_rel_path,
        global,
    };
    save_record(&package_dir, &record)?;

    Ok(record)
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
