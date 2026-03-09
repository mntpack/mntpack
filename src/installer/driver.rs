use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use walkdir::WalkDir;

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

pub fn auto_discover_binary(repo_path: &Path, package_name: &str) -> Result<Option<PathBuf>> {
    let search_roots = [
        repo_path.join("target").join("release"),
        repo_path.join("bin"),
        repo_path.join("dist"),
        repo_path.join("build"),
        repo_path.to_path_buf(),
    ];

    let mut candidates = Vec::new();
    for root in search_roots {
        if !root.exists() {
            continue;
        }
        for entry in WalkDir::new(root).max_depth(5).into_iter().flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path().to_path_buf();
            if !is_executable_candidate(&path)? {
                continue;
            }
            candidates.push(path);
        }
    }

    if candidates.is_empty() {
        return Ok(None);
    }

    if let Some(path) = candidates.iter().find(|path| {
        path.file_stem()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case(package_name))
            .unwrap_or(false)
    }) {
        return Ok(Some(path.clone()));
    }

    if candidates.len() == 1 {
        return Ok(Some(candidates.remove(0)));
    }

    candidates.sort();
    Ok(candidates.first().cloned())
}

fn is_executable_candidate(path: &Path) -> Result<bool> {
    if cfg!(windows) {
        return Ok(path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("exe"))
            .unwrap_or(false));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path)?.permissions().mode();
        return Ok(mode & 0o111 != 0);
    }

    #[allow(unreachable_code)]
    Ok(false)
}
