use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};

use crate::{config::RuntimeContext, package::manifest::Manifest};

#[derive(Debug, Clone)]
pub struct InstallContext {
    pub package_name: String,
    pub repo_path: PathBuf,
    pub package_dir: PathBuf,
    pub manifest: Option<Manifest>,
}

pub struct DriverRuntime<'a> {
    pub runtime: &'a RuntimeContext,
}

#[derive(Debug, Clone)]
pub struct InstallResult {
    pub binary_path: Option<PathBuf>,
    pub shim_name: String,
}

pub trait InstallDriver: Send + Sync {
    fn detect(&self, repo_path: &Path) -> bool;
    fn install(&self, ctx: &InstallContext, runtime: &DriverRuntime<'_>) -> Result<InstallResult>;
}

pub fn run_command(program: &str, args: &[&str], cwd: &Path) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .status()
        .with_context(|| format!("failed to start '{program}' in {}", cwd.display()))?;

    if !status.success() {
        bail!(
            "command '{}' failed with exit code {:?}",
            format_command(program, args),
            status.code()
        );
    }

    Ok(())
}

pub fn run_shell_command(command: &str, cwd: &Path) -> Result<()> {
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", command]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", command]);
        c
    };
    let status = cmd
        .current_dir(cwd)
        .status()
        .with_context(|| format!("failed to run script '{command}' in {}", cwd.display()))?;
    if !status.success() {
        bail!(
            "script '{command}' failed with exit code {:?}",
            status.code()
        );
    }
    Ok(())
}

pub fn manifest_bin(ctx: &InstallContext) -> Result<PathBuf> {
    let Some(manifest) = &ctx.manifest else {
        bail!("mntpack.json is required to determine install binary");
    };
    let Some(bin) = manifest.resolve_bin_path() else {
        bail!("mntpack.json missing required 'bin' field");
    };
    Ok(ctx.repo_path.join(bin))
}

fn format_command(program: &str, args: &[&str]) -> String {
    if args.is_empty() {
        return program.to_string();
    }
    format!("{program} {}", args.join(" "))
}
