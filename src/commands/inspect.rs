use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde::Deserialize;
use walkdir::WalkDir;

use crate::{
    config::RuntimeContext,
    dotnet,
    github::clone::sync_repo,
    package::{manifest::Manifest, resolver::resolve_repo},
};

pub fn execute(runtime: &RuntimeContext, repo_input: &str) -> Result<()> {
    let resolved = resolve_repo(repo_input, &runtime.config.default_owner)?;
    let repo_dir = runtime
        .paths
        .repo_dir_existing_or_new(&resolved.owner, &resolved.repo);

    sync_repo(
        &resolved,
        &repo_dir,
        &runtime.paths.cache_git,
        &runtime.config.paths.git,
        None,
    )?;

    let manifest = Manifest::load(&repo_dir)?;
    let project_type = detect_project_type(&repo_dir);
    let build_hint = build_hint(&repo_dir, &manifest, runtime);
    let run_hint = manifest
        .as_ref()
        .and_then(|m| m.resolve_run_command())
        .or_else(|| {
            manifest
                .as_ref()
                .and_then(|m| m.resolve_bin_command())
                .map(|(_, command)| command)
        });
    let binary_hint = manifest
        .as_ref()
        .and_then(|m| m.resolve_bin_path())
        .map(|path| path.to_string())
        .or_else(|| infer_binary_hint(&repo_dir, &resolved.repo).ok().flatten());
    let release_hint = manifest
        .as_ref()
        .map(|m| !m.release.is_empty())
        .unwrap_or(false);

    println!("Repository: {}", resolved.key);
    println!("Detected project type: {project_type}");
    println!("Build: {build_hint}");
    println!("Run: {}", run_hint.as_deref().unwrap_or("(none)"));
    println!("Binary: {}", binary_hint.as_deref().unwrap_or("(unknown)"));
    println!(
        "Releases: {}",
        if release_hint {
            "manifest release mapping present"
        } else {
            "no explicit manifest release mapping (use --release auto to attempt detection)"
        }
    );

    Ok(())
}

fn detect_project_type(repo_dir: &Path) -> &'static str {
    if repo_dir.join("Cargo.toml").exists() {
        return "Rust";
    }
    if repo_dir.join("pyproject.toml").exists() || repo_dir.join("requirements.txt").exists() {
        return "Python";
    }
    if repo_dir.join("package.json").exists() {
        return "Node";
    }
    if dotnet::is_dotnet_project(repo_dir) {
        return ".NET";
    }
    if repo_dir.join("CMakeLists.txt").exists() {
        return "C++ (CMake)";
    }
    if repo_dir.join("Makefile").exists() || repo_dir.join("makefile").exists() {
        return "C/C++ (Make)";
    }
    "Generic"
}

fn build_hint(repo_dir: &Path, manifest: &Option<Manifest>, runtime: &RuntimeContext) -> String {
    if let Some(build) = manifest.as_ref().and_then(|m| m.build.as_deref()) {
        return build.to_string();
    }
    if repo_dir.join("Cargo.toml").exists() {
        return format!("{} build --release", runtime.config.paths.cargo);
    }
    if repo_dir.join("requirements.txt").exists() {
        return format!("{} install -r requirements.txt", runtime.config.paths.pip);
    }
    if repo_dir.join("package.json").exists() {
        return format!("{} install", runtime.config.paths.npm);
    }
    if let Ok(Some(build)) = dotnet::build_hint(repo_dir, &runtime.config.paths.dotnet) {
        return build;
    }
    if repo_dir.join("CMakeLists.txt").exists() {
        return format!(
            "{} -S . -B .mntpack-build -DCMAKE_BUILD_TYPE=Release && {} --build .mntpack-build --config Release",
            runtime.config.paths.cmake, runtime.config.paths.cmake
        );
    }
    if repo_dir.join("Makefile").exists() || repo_dir.join("makefile").exists() {
        return runtime.config.paths.make.to_string();
    }
    "(none)".to_string()
}

fn infer_binary_hint(repo_dir: &Path, repo_name: &str) -> Result<Option<String>> {
    if repo_dir.join("Cargo.toml").exists() {
        let expected = expected_rust_binary_name(repo_dir).ok().flatten();
        if let Some(name) = expected {
            let rel = format!("target/release/{name}");
            return Ok(Some(rel));
        }
        return Ok(Some(format!("target/release/{repo_name}")));
    }

    let common_roots = [
        repo_dir.join("target").join("release"),
        repo_dir.join("bin"),
        repo_dir.join("dist"),
        repo_dir.join("build"),
        repo_dir.to_path_buf(),
    ];

    for root in common_roots {
        if !root.exists() {
            continue;
        }
        let mut matches = Vec::new();
        for entry in WalkDir::new(&root).max_depth(4).into_iter().flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if is_executable_candidate(path)? {
                matches.push(path.to_path_buf());
            }
        }
        if matches.len() == 1 {
            let rel = matches[0]
                .strip_prefix(repo_dir)
                .unwrap_or(&matches[0])
                .to_string_lossy()
                .replace('\\', "/");
            return Ok(Some(rel));
        }
    }

    Ok(None)
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CargoToml {
    package: Option<CargoPackage>,
}

fn expected_rust_binary_name(repo_dir: &Path) -> Result<Option<String>> {
    let cargo_toml = repo_dir.join("Cargo.toml");
    if !cargo_toml.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&cargo_toml)
        .with_context(|| format!("failed to read {}", cargo_toml.display()))?;
    let parsed = toml::from_str::<CargoToml>(&content)
        .with_context(|| format!("failed to parse {}", cargo_toml.display()))?;
    let Some(name) = parsed.package.and_then(|p| p.name) else {
        return Ok(None);
    };
    let mut executable_name = name.replace('-', "_");
    if cfg!(windows) {
        executable_name.push_str(".exe");
    }
    Ok(Some(executable_name))
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
