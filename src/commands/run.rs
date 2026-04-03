use std::{collections::HashSet, path::PathBuf, process::Command};

use anyhow::{Context, Result, bail};

use crate::{
    config::RuntimeContext, installer::driver::run_command_with_args, package::record::load_record,
};

pub async fn execute(runtime: &RuntimeContext, package_name: &str, args: &[String]) -> Result<()> {
    let package_dir = runtime.paths.package_dir(package_name);
    if !package_dir.exists() {
        bail!("package '{package_name}' is not installed");
    }

    if runtime.config.auto_update_on_run {
        if let Some(record) = load_record(&package_dir)? {
            let mut visited = HashSet::new();
            crate::commands::sync::sync_package_internal(
                runtime,
                &record.repo_spec(),
                record.version.as_deref(),
                None,
                Some(&record.package_name),
                record.global,
                &mut visited,
            )
            .await?;
        }
    }

    let record = crate::commands::sync::ensure_package_ready(runtime, package_name).await?;

    if let Some(run_command) = record.run_command.as_deref() {
        return execute_run_command(runtime, &record, run_command, args);
    }

    let binary_path =
        crate::commands::sync::resolve_binary_path(runtime, &record).unwrap_or_else(|| {
            if cfg!(windows) {
                package_dir.join(format!("{package_name}.exe"))
            } else {
                package_dir.join(package_name)
            }
        });

    if !binary_path.exists() {
        bail!(
            "package binary for '{package_name}' not found at {}",
            binary_path.display()
        );
    }

    let status = Command::new(&binary_path)
        .args(args)
        .status()
        .with_context(|| format!("failed to launch {}", binary_path.display()))?;
    if !status.success() {
        bail!(
            "package '{}' exited with status {:?}",
            package_name,
            status.code()
        );
    }
    Ok(())
}

fn execute_run_command(
    runtime: &RuntimeContext,
    record: &crate::package::record::PackageRecord,
    base_command: &str,
    args: &[String],
) -> Result<()> {
    let invocation_cwd = std::env::current_dir().context("failed to resolve current directory")?;
    let repo_dir = runtime
        .paths
        .repo_dir_existing_or_new(&record.owner, &record.repo);
    if !repo_dir.exists() {
        bail!(
            "repository directory not found for '{}'",
            record.package_name
        );
    }

    if let Some(executable) = resolve_repo_local_executable(&repo_dir, base_command) {
        let status = Command::new(&executable)
            .args(args)
            .status()
            .with_context(|| format!("failed to launch {}", executable.display()))?;
        if !status.success() {
            bail!(
                "package '{}' exited with status {:?}",
                record.package_name,
                status.code()
            );
        }
        return Ok(());
    }

    run_command_with_args(base_command, args, &repo_dir, &invocation_cwd)
}

fn resolve_repo_local_executable(repo_dir: &std::path::Path, command: &str) -> Option<PathBuf> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains(' ') || trimmed.contains('\t') {
        return None;
    }

    let relative = if trimmed.starts_with("./") || trimmed.starts_with(".\\") {
        &trimmed[2..]
    } else if trimmed.contains('/') || trimmed.contains('\\') {
        trimmed
    } else {
        return None;
    };
    if relative.is_empty() {
        return None;
    }

    let candidate = repo_dir.join(relative.replace('\\', "/"));
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}
