use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use walkdir::WalkDir;

use crate::{
    config::RuntimeContext,
    github::{clone::sync_repo, release::try_download_release_binary},
    installer::{
        driver::{DriverRuntime, InstallContext, run_shell_command},
        manager::{InstallerManager, materialize_binary},
    },
    package::{
        manifest::Manifest, record::find_record_by_package_name, resolver::resolve_repo,
        store::version_store_dir,
    },
};

pub async fn execute(runtime: &RuntimeContext, repo_input: &str, args: &[String]) -> Result<()> {
    if let Some((package, version)) = parse_versioned_package(repo_input) {
        return execute_stored_version(runtime, &package, &version, args);
    }

    let resolved = resolve_repo(repo_input, &runtime.config.default_owner)?;
    let suffix = unique_suffix();
    let exec_root = runtime
        .paths
        .cache_exec
        .join(format!("{}-{suffix}", resolved.key));
    let _cleanup = TempExecDir {
        path: exec_root.clone(),
    };

    let repo_dir = exec_root.join("repo");
    let package_dir = exec_root.join("package");
    fs::create_dir_all(&repo_dir)
        .with_context(|| format!("failed to create {}", repo_dir.display()))?;
    fs::create_dir_all(&package_dir)
        .with_context(|| format!("failed to create {}", package_dir.display()))?;

    sync_repo(
        &resolved,
        &repo_dir,
        &runtime.paths.cache_git,
        &runtime.config.paths.git,
        None,
    )?;
    let manifest = Manifest::load(&repo_dir)?;
    let run_command = manifest
        .as_ref()
        .and_then(|m| m.resolve_run_command())
        .or_else(|| {
            manifest
                .as_ref()
                .and_then(|m| m.resolve_bin_command())
                .map(|(_, command)| command)
        });

    let runtime_driver = DriverRuntime { runtime };
    let installer_ctx = InstallContext {
        package_name: resolved.repo.clone(),
        repo_path: repo_dir.clone(),
        package_dir: package_dir.clone(),
        manifest: manifest.clone(),
    };

    let mut installed_binary = None;
    if run_command.is_none() {
        if let Some(manifest) = &manifest {
            if let Some(release_binary) =
                try_download_release_binary(runtime, &resolved, Some(manifest), None, None).await?
            {
                installed_binary = Some(materialize_binary(
                    &release_binary,
                    &package_dir,
                    &resolved.repo,
                )?);
            }
        }
    }

    if installed_binary.is_none() || run_command.is_some() {
        let result = InstallerManager::new().install(&installer_ctx, &runtime_driver)?;
        if let Some(binary) = result.binary_path {
            installed_binary = Some(binary);
        }
    }

    if let Some(command) = run_command {
        let full_command = append_args(&command, args);
        run_shell_command(&full_command, &repo_dir)?;
        return Ok(());
    }

    let Some(binary_path) = installed_binary else {
        bail!(
            "unable to determine executable for repository '{}'",
            resolved.key
        );
    };

    let status = Command::new(&binary_path)
        .args(args)
        .status()
        .with_context(|| format!("failed to launch {}", binary_path.display()))?;
    if !status.success() {
        bail!(
            "ephemeral exec for '{}' exited with status {:?}",
            resolved.key,
            status.code()
        );
    }

    Ok(())
}

struct TempExecDir {
    path: std::path::PathBuf,
}

impl Drop for TempExecDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn unique_suffix() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    millis.to_string()
}

fn append_args(base_command: &str, args: &[String]) -> String {
    if args.is_empty() {
        return base_command.to_string();
    }
    let escaped: Vec<String> = args.iter().map(|arg| shell_escape(arg)).collect();
    format!("{base_command} {}", escaped.join(" "))
}

fn shell_escape(input: &str) -> String {
    if cfg!(windows) {
        format!("\"{}\"", input.replace('"', "\\\""))
    } else {
        format!("'{}'", input.replace('\'', "'\"'\"'"))
    }
}

fn parse_versioned_package(input: &str) -> Option<(String, String)> {
    let trimmed = input.trim();
    let (package, version) = trimmed.rsplit_once('@')?;
    if package.is_empty() || version.is_empty() {
        return None;
    }
    if package.contains('/') || package.contains("://") {
        return None;
    }
    Some((package.to_string(), version.to_string()))
}

fn execute_stored_version(
    runtime: &RuntimeContext,
    package_name: &str,
    version: &str,
    args: &[String],
) -> Result<()> {
    let Some(record) = find_record_by_package_name(&runtime.paths.packages, package_name)? else {
        bail!("package '{package_name}' is not installed");
    };

    let store_dir = version_store_dir(&runtime.paths.store, &record.repo, version);
    if !store_dir.exists() {
        bail!(
            "version '{}' is not installed for package '{}': {}",
            version,
            package_name,
            store_dir.display()
        );
    }

    let preferred_binary = record
        .binary_path
        .as_deref()
        .and_then(|value| Path::new(value).file_name().and_then(|v| v.to_str()))
        .map(ToString::to_string)
        .or_else(|| {
            record
                .binary_rel_path
                .as_deref()
                .and_then(|value| Path::new(value).file_name().and_then(|v| v.to_str()))
                .map(ToString::to_string)
        });
    let binary = select_binary_from_store(&store_dir, preferred_binary.as_deref())?;

    let status = Command::new(&binary)
        .args(args)
        .status()
        .with_context(|| format!("failed to launch {}", binary.display()))?;
    if !status.success() {
        bail!(
            "exec for '{}@{}' exited with status {:?}",
            package_name,
            version,
            status.code()
        );
    }

    Ok(())
}

fn select_binary_from_store(store_dir: &Path, preferred_name: Option<&str>) -> Result<PathBuf> {
    if let Some(name) = preferred_name {
        let direct = store_dir.join(name);
        if direct.exists() && is_executable_candidate(&direct)? {
            return Ok(direct);
        }
    }

    let mut candidates = Vec::new();
    for entry in WalkDir::new(store_dir).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path().to_path_buf();
        if !is_executable_candidate(&path)? {
            continue;
        }
        candidates.push(path);
    }

    match candidates.len() {
        0 => bail!("no executable binary found in {}", store_dir.display()),
        1 => Ok(candidates.remove(0)),
        _ => {
            candidates.sort();
            Ok(candidates.remove(0))
        }
    }
}

fn is_executable_candidate(path: &Path) -> Result<bool> {
    if cfg!(windows) {
        let ext = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
        return Ok(ext.eq_ignore_ascii_case("exe"));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path)
            .with_context(|| format!("failed to read metadata for {}", path.display()))?
            .permissions()
            .mode();
        return Ok(mode & 0o111 != 0);
    }

    #[allow(unreachable_code)]
    Ok(false)
}
