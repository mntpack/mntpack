use std::{
    env,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::{
    cli::NugetAction,
    config::RuntimeContext,
    dotnet,
    package::manifest::{Manifest, NugetPackage, remove_nuget_package, upsert_nuget_package},
};

pub fn execute(runtime: &RuntimeContext, action: NugetAction) -> Result<()> {
    match action {
        NugetAction::Add {
            package,
            version,
            source,
            path,
            project,
            build,
        } => add_package(
            runtime,
            &package,
            version.as_deref(),
            source.as_deref(),
            path.as_deref(),
            project.as_deref(),
            build,
        ),
        NugetAction::Remove {
            package,
            path,
            project,
            build,
        } => remove_package(
            runtime,
            &package,
            path.as_deref(),
            project.as_deref(),
            build,
        ),
        NugetAction::List { path } => list_packages(&resolve_root(path.as_deref())?),
        NugetAction::Install {
            path,
            project,
            build,
        }
        | NugetAction::Apply {
            path,
            project,
            build,
        } => install_from_manifest(runtime, path.as_deref(), project.as_deref(), build),
        NugetAction::Restore {
            path,
            project,
            build,
        } => restore_project(runtime, path.as_deref(), project.as_deref(), build),
        NugetAction::Ensure { path, project } => {
            ensure_feed(runtime, path.as_deref(), project.as_deref())
        }
    }
}

fn add_package(
    runtime: &RuntimeContext,
    package_id: &str,
    version: Option<&str>,
    source: Option<&str>,
    path: Option<&Path>,
    project: Option<&Path>,
    build: bool,
) -> Result<()> {
    let root = resolve_root(path)?;
    let package = NugetPackage {
        id: package_id.trim().to_string(),
        version: version
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        source: source
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    };
    if package.id.is_empty() {
        bail!("package id cannot be empty");
    }

    let changed = upsert_nuget_package(&root, &package)?;
    if !dotnet::is_dotnet_project(&root) {
        println!(
            "updated mntpack.json at {}{}",
            root.display(),
            if changed { "" } else { " (no changes)" }
        );
        println!("no .NET project was detected, so only the manifest was updated");
        return Ok(());
    }

    let config = dotnet::ensure_workspace_config(runtime, &root, project)?;
    let selected_project = dotnet::add_package_reference(runtime, &root, project, &package)?;
    dotnet::restore(runtime, &root, project)?;
    if build {
        dotnet::build(runtime, &root)?;
    }

    println!("manifest: {}", root.join("mntpack.json").display());
    println!("project: {}", selected_project.display());
    println!("config: {}", config.path.display());
    println!("source: {} = {}", config.source_key, config.source_value);
    println!(
        "added package {}{}{}",
        package.id,
        package
            .version
            .as_deref()
            .map(|value| format!(" {}", value))
            .unwrap_or_default(),
        if changed {
            ""
        } else {
            " (manifest already up to date)"
        }
    );
    Ok(())
}

fn remove_package(
    runtime: &RuntimeContext,
    package_id: &str,
    path: Option<&Path>,
    project: Option<&Path>,
    build: bool,
) -> Result<()> {
    let root = resolve_root(path)?;
    let manifest_changed = remove_nuget_package(&root, package_id)?;

    if !dotnet::is_dotnet_project(&root) {
        if manifest_changed {
            println!(
                "removed package {} from {}",
                package_id,
                root.join("mntpack.json").display()
            );
        } else {
            println!("package {} was not present in mntpack.json", package_id);
        }
        println!("no .NET project was detected, so no project file was changed");
        return Ok(());
    }

    let config = dotnet::ensure_workspace_config(runtime, &root, project)?;
    let selected_project = dotnet::remove_package_reference(runtime, &root, project, package_id)?;
    dotnet::restore(runtime, &root, project)?;
    if build {
        dotnet::build(runtime, &root)?;
    }

    println!("manifest: {}", root.join("mntpack.json").display());
    println!("project: {}", selected_project.display());
    println!("config: {}", config.path.display());
    println!("removed package {}", package_id);
    if !manifest_changed {
        println!("mntpack.json was already missing that package entry");
    }
    Ok(())
}

fn list_packages(root: &Path) -> Result<()> {
    let manifest = Manifest::load(root)?;
    let packages = manifest
        .as_ref()
        .map(Manifest::resolved_nuget_packages)
        .unwrap_or_default();

    if packages.is_empty() {
        println!(
            "no NuGet packages are declared in {}",
            root.join("mntpack.json").display()
        );
        return Ok(());
    }

    println!("NuGet packages in {}:", root.join("mntpack.json").display());
    for package in packages {
        let version = package.version.unwrap_or_else(|| "(latest)".to_string());
        let source = package.source.unwrap_or_else(|| "(default)".to_string());
        println!("- {} | version {} | source {}", package.id, version, source);
    }
    Ok(())
}

fn install_from_manifest(
    runtime: &RuntimeContext,
    path: Option<&Path>,
    project: Option<&Path>,
    build: bool,
) -> Result<()> {
    let root = resolve_root(path)?;
    let manifest = require_manifest(&root)?;
    let config = dotnet::ensure_workspace_config(runtime, &root, project)?;
    let applied = dotnet::apply_manifest_packages(runtime, &root, project, &manifest)?;
    if applied.is_empty() {
        println!(
            "no NuGet packages are declared in {}",
            root.join("mntpack.json").display()
        );
        return Ok(());
    }

    if build {
        dotnet::build(runtime, &root)?;
    }

    println!("manifest: {}", root.join("mntpack.json").display());
    println!("config: {}", config.path.display());
    println!("applied {} package(s)", applied.len());
    Ok(())
}

fn restore_project(
    runtime: &RuntimeContext,
    path: Option<&Path>,
    project: Option<&Path>,
    build: bool,
) -> Result<()> {
    let root = resolve_root(path)?;
    let config = dotnet::ensure_workspace_config(runtime, &root, project)?;
    dotnet::restore(runtime, &root, project)?;
    if build {
        dotnet::build(runtime, &root)?;
    }

    println!("config: {}", config.path.display());
    println!("restored .NET dependencies in {}", root.display());
    Ok(())
}

fn ensure_feed(
    runtime: &RuntimeContext,
    path: Option<&Path>,
    project: Option<&Path>,
) -> Result<()> {
    let root = resolve_root(path)?;
    let update = dotnet::ensure_workspace_config(runtime, &root, project)?;
    println!("config: {}", update.path.display());
    println!("source: {} = {}", update.source_key, update.source_value);
    println!("local feed: {}", runtime.paths.nuget_source.display());
    println!(
        "{}",
        if update.changed {
            "NuGet.config updated"
        } else {
            "NuGet.config already contained the mntpack source"
        }
    );
    Ok(())
}

fn resolve_root(path: Option<&Path>) -> Result<PathBuf> {
    let path = match path {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => env::current_dir()
            .context("failed to resolve current directory")?
            .join(path),
        None => env::current_dir().context("failed to resolve current directory")?,
    };

    if !path.exists() {
        bail!("path not found at {}", path.display());
    }
    if !path.is_dir() {
        bail!("path must point to a directory");
    }
    Ok(path)
}

fn require_manifest(root: &Path) -> Result<Manifest> {
    let Some(manifest) = Manifest::load(root)? else {
        bail!("mntpack.json was not found in {}", root.display());
    };
    Ok(manifest)
}
