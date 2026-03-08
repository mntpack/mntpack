use std::{collections::HashSet, process::Command};

use anyhow::{Context, Result, bail};

use crate::{
    config::RuntimeContext, installer::driver::run_shell_command, package::record::load_record,
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
    let repo_key = crate::config::repo_key(&record.owner, &record.repo);
    let repo_dir = runtime.paths.repo_dir(&repo_key);
    if !repo_dir.exists() {
        bail!(
            "repository directory not found for '{}'",
            record.package_name
        );
    }

    let command = append_args(base_command, args);
    run_shell_command(&command, &repo_dir)
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
