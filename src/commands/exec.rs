use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};

use crate::{
    config::RuntimeContext,
    github::{clone::sync_repo, release::try_download_release_binary},
    installer::{
        driver::{DriverRuntime, InstallContext, run_shell_command},
        manager::{InstallerManager, materialize_binary},
    },
    package::{manifest::Manifest, resolver::resolve_repo},
};

pub async fn execute(runtime: &RuntimeContext, repo_input: &str, args: &[String]) -> Result<()> {
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

    sync_repo(&resolved, &repo_dir, &runtime.paths.cache_git, None)?;
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
                try_download_release_binary(runtime, &resolved, manifest, None, None).await?
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
