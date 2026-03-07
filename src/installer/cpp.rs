use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use anyhow::{Context, Result, bail};
use walkdir::WalkDir;

use super::driver::{DriverRuntime, InstallContext, InstallDriver, InstallResult, run_command};

pub struct CppDriver;

impl InstallDriver for CppDriver {
    fn detect(&self, repo_path: &Path) -> bool {
        repo_path.join("CMakeLists.txt").exists()
            || repo_path.join("Makefile").exists()
            || repo_path.join("makefile").exists()
    }

    fn install(&self, ctx: &InstallContext, runtime: &DriverRuntime<'_>) -> Result<InstallResult> {
        let cmake_lists = ctx.repo_path.join("CMakeLists.txt");
        let use_cmake = cmake_lists.exists();
        let binary = if use_cmake {
            build_with_cmake(ctx, runtime)?
        } else if ctx.repo_path.join("Makefile").exists() || ctx.repo_path.join("makefile").exists()
        {
            build_with_make(ctx, runtime)?
        } else {
            bail!("cpp driver detected project but no cmake/make build file was found");
        };

        let shim_name = binary
            .file_stem()
            .and_then(|name| name.to_str())
            .map(|name| name.to_string())
            .unwrap_or_else(|| ctx.package_name.clone());

        Ok(InstallResult {
            binary_path: Some(binary),
            shim_name,
        })
    }
}

fn build_with_cmake(ctx: &InstallContext, runtime: &DriverRuntime<'_>) -> Result<PathBuf> {
    let build_dir = ctx.repo_path.join(".mntpack-build");
    let cmake = &runtime.runtime.config.paths.cmake;
    run_command(
        cmake,
        &[
            "-S",
            ".",
            "-B",
            ".mntpack-build",
            "-DCMAKE_BUILD_TYPE=Release",
        ],
        &ctx.repo_path,
    )?;
    run_command(
        cmake,
        &["--build", ".mntpack-build", "--config", "Release"],
        &ctx.repo_path,
    )?;

    detect_binary(&[build_dir.join("Release"), build_dir], &ctx.package_name)
}

fn build_with_make(ctx: &InstallContext, runtime: &DriverRuntime<'_>) -> Result<PathBuf> {
    let make = &runtime.runtime.config.paths.make;
    run_command(make, &[], &ctx.repo_path)?;
    detect_binary(&[ctx.repo_path.clone()], &ctx.package_name)
}

fn detect_binary(search_roots: &[PathBuf], package_name: &str) -> Result<PathBuf> {
    let mut candidates: Vec<(PathBuf, SystemTime)> = Vec::new();
    for root in search_roots {
        if !root.exists() {
            continue;
        }
        for entry in WalkDir::new(root).into_iter().flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if path.to_string_lossy().contains("CMakeFiles") {
                continue;
            }
            if !is_executable_candidate(path)? {
                continue;
            }
            let modified = fs::metadata(path)
                .with_context(|| format!("failed to read metadata for {}", path.display()))?
                .modified()
                .unwrap_or(SystemTime::UNIX_EPOCH);
            candidates.push((path.to_path_buf(), modified));
        }
    }

    if candidates.is_empty() {
        bail!("cpp build succeeded but no executable binary was detected");
    }

    if let Some((path, _)) = candidates.iter().find(|(path, _)| {
        path.file_stem()
            .and_then(|name| name.to_str())
            .map(|name| name.eq_ignore_ascii_case(package_name))
            .unwrap_or(false)
    }) {
        return Ok(path.clone());
    }

    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(candidates.remove(0).0)
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
        let mode = fs::metadata(path)
            .with_context(|| format!("failed to read metadata for {}", path.display()))?
            .permissions()
            .mode();
        return Ok(mode & 0o111 != 0);
    }

    #[allow(unreachable_code)]
    Ok(false)
}
