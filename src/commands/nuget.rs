use std::{
    env,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::{
    cli::{NugetAction, NugetCacheAction, NugetFeedAction, NugetSourceAction},
    config::RuntimeContext,
    dotnet, nuget,
    package::manifest::{
        Manifest, NugetPackage, NugetSourceDefinition, manifest_path, remove_nuget_package,
        upsert_nuget_package, upsert_nuget_source,
    },
};

pub fn execute(runtime: &RuntimeContext, action: NugetAction) -> Result<()> {
    match action {
        NugetAction::Init { path, project } => {
            init_consumer(runtime, path.as_deref(), project.as_deref())
        }
        NugetAction::Feed { action } => execute_feed(runtime, action),
        NugetAction::Cache { action } => execute_cache(action),
        NugetAction::Source { action } => execute_source(runtime, action),
        NugetAction::Add {
            package,
            version,
            source,
            path,
            project,
            no_restore,
            refresh,
            build,
        } => add_package(
            runtime,
            &package,
            version.as_deref(),
            source.as_deref(),
            path.as_deref(),
            project.as_deref(),
            no_restore,
            refresh,
            build,
        ),
        NugetAction::Use {
            package,
            version,
            source,
            path,
            project,
            no_restore,
            refresh,
            build,
        } => add_package(
            runtime,
            &package,
            version.as_deref(),
            source.as_deref().or(Some(nuget::MNTPACK_LOCAL_SOURCE_KEY)),
            path.as_deref(),
            project.as_deref(),
            no_restore,
            refresh,
            build,
        ),
        NugetAction::Remove {
            package,
            path,
            project,
            no_restore,
            build,
        } => remove_package(
            runtime,
            &package,
            path.as_deref(),
            project.as_deref(),
            no_restore,
            build,
        ),
        NugetAction::List { path, project } => list_packages(path.as_deref(), project.as_deref()),
        NugetAction::Apply {
            path,
            project,
            refresh,
            build,
        } => install_from_manifest(runtime, path.as_deref(), project.as_deref(), refresh, build),
        NugetAction::Restore {
            path,
            project,
            refresh,
            build,
        } => restore_project(runtime, path.as_deref(), project.as_deref(), refresh, build),
        NugetAction::Refresh {
            path,
            project,
            force,
            build,
        } => refresh_workspace(runtime, path.as_deref(), project.as_deref(), force, build),
    }
}

fn execute_feed(runtime: &RuntimeContext, action: NugetFeedAction) -> Result<()> {
    match action {
        NugetFeedAction::Path => {
            let path = nuget::ensure_feed(runtime)?;
            println!("{}", path.display());
        }
        NugetFeedAction::List => {
            let packages = nuget::list_feed_packages(runtime)?;
            if packages.is_empty() {
                println!(
                    "no packages are currently stored in {}",
                    runtime.paths.nuget_feed.display()
                );
                return Ok(());
            }
            println!("NuGet feed at {}:", runtime.paths.nuget_feed.display());
            for package in packages {
                println!(
                    "- {} | version {} | {}",
                    package.package_id,
                    package.version,
                    package.path.display()
                );
            }
        }
    }
    Ok(())
}

fn execute_cache(action: NugetCacheAction) -> Result<()> {
    match action {
        NugetCacheAction::Clear { package, version } => {
            let removed = nuget::clear_global_package_cache(&package, version.as_deref())?;
            if removed.is_empty() {
                println!(
                    "no cached NuGet package entries matched {}{}",
                    package,
                    version
                        .as_deref()
                        .map(|value| format!(" {}", value))
                        .unwrap_or_default()
                );
            } else {
                for path in removed {
                    println!("removed {}", path.display());
                }
            }
        }
    }
    Ok(())
}

fn execute_source(runtime: &RuntimeContext, action: NugetSourceAction) -> Result<()> {
    match action {
        NugetSourceAction::Add {
            name,
            repo,
            reference,
            subdir,
            project,
            solution,
            package_id,
            version,
            configuration,
            path,
            auto_build,
        } => add_source(
            runtime,
            &name,
            &repo,
            reference.as_deref(),
            subdir.as_deref(),
            project.as_deref(),
            solution.as_deref(),
            package_id.as_deref(),
            version.as_deref(),
            configuration.as_deref(),
            path.as_deref(),
            auto_build,
        ),
        NugetSourceAction::List { path } => list_sources(runtime, path.as_deref()),
        NugetSourceAction::Build { name, path, force } => {
            build_source(runtime, &name, path.as_deref(), force)
        }
        NugetSourceAction::BuildAll { path, force } => {
            build_all_sources(runtime, path.as_deref(), force)
        }
        NugetSourceAction::Update { name, path } => update_source(runtime, &name, path.as_deref()),
        NugetSourceAction::Sync { path, force } => sync_sources(runtime, path.as_deref(), force),
    }
}

fn init_consumer(
    runtime: &RuntimeContext,
    path: Option<&Path>,
    project: Option<&Path>,
) -> Result<()> {
    let root = resolve_consumer_root(path, project)?;
    let update = dotnet::ensure_workspace_config(runtime, &root, project)?;
    println!("config: {}", update.path.display());
    println!("source: {} = {}", update.source_key, update.source_value);
    println!("local feed: {}", runtime.paths.nuget_feed.display());
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

fn add_package(
    runtime: &RuntimeContext,
    package_id: &str,
    version: Option<&str>,
    source: Option<&str>,
    path: Option<&Path>,
    project: Option<&Path>,
    no_restore: bool,
    refresh: bool,
    build: bool,
) -> Result<()> {
    let root = resolve_consumer_root(path, project)?;
    let manifest_root = resolve_manifest_root(path)?;
    let package_id = package_id.trim();
    if package_id.is_empty() {
        bail!("package id cannot be empty");
    }

    let package = resolve_requested_package(runtime, &manifest_root, package_id, version, source)?;
    let changed = upsert_nuget_package(&manifest_root, &package)?;

    if !dotnet::is_dotnet_project(&root) {
        println!(
            "updated mntpack.json at {}{}",
            manifest_path(&manifest_root).display(),
            if changed { "" } else { " (no changes)" }
        );
        println!("no .NET project was detected, so only the manifest was updated");
        return Ok(());
    }

    if package
        .source
        .as_deref()
        .map(|value| value.eq_ignore_ascii_case(nuget::MNTPACK_LOCAL_SOURCE_KEY))
        .unwrap_or(false)
    {
        let _ = nuget::ensure_source_package_available(
            runtime,
            &manifest_root,
            &package.id,
            package.version.as_deref(),
        )?;
    }

    let config = dotnet::ensure_workspace_config(runtime, &root, project)?;
    if refresh {
        refresh_package_cache(&package)?;
    }
    let selected_project = dotnet::add_package_reference(runtime, &root, project, &package)?;
    if !no_restore {
        dotnet::restore(runtime, &root, project)?;
    }
    if build {
        dotnet::build(runtime, &root)?;
    }

    println!("manifest: {}", manifest_path(&manifest_root).display());
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
    no_restore: bool,
    build: bool,
) -> Result<()> {
    let root = resolve_consumer_root(path, project)?;
    let manifest_root = resolve_manifest_root(path)?;
    let manifest_changed = remove_nuget_package(&manifest_root, package_id)?;

    if !dotnet::is_dotnet_project(&root) {
        if manifest_changed {
            println!(
                "removed package {} from {}",
                package_id,
                manifest_path(&manifest_root).display()
            );
        } else {
            println!("package {} was not present in mntpack.json", package_id);
        }
        println!("no .NET project was detected, so no project file was changed");
        return Ok(());
    }

    let config = dotnet::ensure_workspace_config(runtime, &root, project)?;
    let selected_project = dotnet::remove_package_reference(runtime, &root, project, package_id)?;
    if !no_restore {
        dotnet::restore(runtime, &root, project)?;
    }
    if build {
        dotnet::build(runtime, &root)?;
    }

    println!("manifest: {}", manifest_path(&manifest_root).display());
    println!("project: {}", selected_project.display());
    println!("config: {}", config.path.display());
    println!("removed package {}", package_id);
    if !manifest_changed {
        println!("mntpack.json was already missing that package entry");
    }
    Ok(())
}

fn list_packages(path: Option<&Path>, project: Option<&Path>) -> Result<()> {
    let manifest_root = resolve_manifest_root(path)?;
    let manifest = Manifest::load(&manifest_root)?;
    let manifest_packages = manifest
        .as_ref()
        .map(Manifest::resolved_nuget_packages)
        .unwrap_or_default();
    let consumer_root = resolve_consumer_root(path, project)?;
    let project_packages = if dotnet::is_dotnet_project(&consumer_root) {
        nuget::list_project_packages(&consumer_root, project)?
    } else {
        Vec::new()
    };

    if manifest_packages.is_empty() && project_packages.is_empty() {
        println!(
            "no NuGet packages were found in {} or the current project",
            manifest_path(&manifest_root).display()
        );
        return Ok(());
    }

    if !manifest_packages.is_empty() {
        println!(
            "Configured packages in {}:",
            manifest_path(&manifest_root).display()
        );
        for package in manifest_packages {
            let version = package.version.unwrap_or_else(|| "(latest)".to_string());
            let source = package.source.unwrap_or_else(|| "(default)".to_string());
            println!("- {} | version {} | source {}", package.id, version, source);
        }
    }

    if !project_packages.is_empty() {
        println!("Package references in the current project:");
        for package in project_packages {
            println!(
                "- {} | version {}",
                package.name,
                package
                    .version
                    .unwrap_or_else(|| "(managed elsewhere)".to_string())
            );
        }
    }

    Ok(())
}

fn install_from_manifest(
    runtime: &RuntimeContext,
    path: Option<&Path>,
    project: Option<&Path>,
    refresh: bool,
    build: bool,
) -> Result<()> {
    let root = resolve_consumer_root(path, project)?;
    let manifest_root = resolve_manifest_root(path)?;
    let manifest = require_manifest(&manifest_root)?;
    let packages = manifest.resolved_nuget_packages();
    if packages.is_empty() {
        println!(
            "no NuGet packages are declared in {}",
            manifest_path(&manifest_root).display()
        );
        return Ok(());
    }

    let config = dotnet::ensure_workspace_config(runtime, &root, project)?;
    if refresh {
        refresh_packages_cache(&packages)?;
    }
    for package in &packages {
        if package
            .source
            .as_deref()
            .map(|value| value.eq_ignore_ascii_case(nuget::MNTPACK_LOCAL_SOURCE_KEY))
            .unwrap_or(false)
        {
            let _ = nuget::ensure_source_package_available(
                runtime,
                &manifest_root,
                &package.id,
                package.version.as_deref(),
            )?;
        }
        dotnet::add_package_reference(runtime, &root, project, package)?;
    }
    dotnet::restore(runtime, &root, project)?;
    if build {
        dotnet::build(runtime, &root)?;
    }

    println!("manifest: {}", manifest_path(&manifest_root).display());
    println!("config: {}", config.path.display());
    println!("applied {} package(s)", packages.len());
    Ok(())
}

fn restore_project(
    runtime: &RuntimeContext,
    path: Option<&Path>,
    project: Option<&Path>,
    refresh: bool,
    build: bool,
) -> Result<()> {
    let root = resolve_consumer_root(path, project)?;
    let config = dotnet::ensure_workspace_config(runtime, &root, project)?;
    if refresh {
        if let Ok(manifest) = require_manifest(&resolve_manifest_root(path)?) {
            refresh_packages_cache(&manifest.resolved_nuget_packages())?;
        }
    }
    dotnet::restore(runtime, &root, project)?;
    if build {
        dotnet::build(runtime, &root)?;
    }

    println!("config: {}", config.path.display());
    println!("restored .NET dependencies in {}", root.display());
    Ok(())
}

fn refresh_workspace(
    runtime: &RuntimeContext,
    path: Option<&Path>,
    project: Option<&Path>,
    force: bool,
    build: bool,
) -> Result<()> {
    let manifest_root = resolve_manifest_root(path)?;
    let manifest = require_manifest(&manifest_root)?;
    let packages = manifest.resolved_nuget_packages();
    let local_packages: Vec<_> = packages
        .iter()
        .filter(|package| {
            package
                .source
                .as_deref()
                .map(|value| value.eq_ignore_ascii_case(nuget::MNTPACK_LOCAL_SOURCE_KEY))
                .unwrap_or(false)
        })
        .cloned()
        .collect();

    let mut rebuilt_sources = Vec::new();
    if !manifest.nuget_source_definitions().is_empty() {
        rebuilt_sources = nuget::sync_all_sources(runtime, &manifest_root, force)?;
    }

    let root = resolve_consumer_root(path, project)?;
    if dotnet::is_dotnet_project(&root) {
        let config = dotnet::ensure_workspace_config(runtime, &root, project)?;
        refresh_packages_cache(&local_packages)?;
        dotnet::restore(runtime, &root, project)?;
        if build {
            dotnet::build(runtime, &root)?;
        }
        println!("config: {}", config.path.display());
    }

    if rebuilt_sources.is_empty() && local_packages.is_empty() {
        println!(
            "no source-backed or mntpack-local NuGet packages were found in {}",
            manifest_path(&manifest_root).display()
        );
        return Ok(());
    }

    println!("manifest: {}", manifest_path(&manifest_root).display());
    if !rebuilt_sources.is_empty() {
        for result in rebuilt_sources {
            println!(
                "rebuilt {} {} -> {}",
                result.package.package_id,
                result.package.version,
                result.package.path.display()
            );
        }
    }
    if !local_packages.is_empty() {
        println!(
            "refreshed {} local package reference(s)",
            local_packages.len()
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn add_source(
    _runtime: &RuntimeContext,
    name: &str,
    repo: &str,
    reference: Option<&str>,
    subdir: Option<&Path>,
    project: Option<&Path>,
    solution: Option<&Path>,
    package_id: Option<&str>,
    version: Option<&str>,
    configuration: Option<&str>,
    path: Option<&Path>,
    auto_build: bool,
) -> Result<()> {
    let manifest_root = resolve_manifest_root(path)?;
    let source = NugetSourceDefinition {
        source_type: "github".to_string(),
        repo: repo.trim().to_string(),
        reference: clean_str(reference),
        subdir: clean_path(subdir),
        project: clean_path(project),
        solution: clean_path(solution),
        package_id: clean_str(package_id),
        version: clean_str(version),
        configuration: clean_str(configuration),
        output_mode: Some("feed".to_string()),
        auto_build: Some(auto_build),
        auto_update: None,
    };
    if source.repo.trim().is_empty() {
        bail!("--repo is required");
    }

    let changed = upsert_nuget_source(&manifest_root, name.trim(), &source)?;
    println!("manifest: {}", manifest_path(&manifest_root).display());
    println!(
        "{} nuget source '{}'",
        if changed { "updated" } else { "kept" },
        name.trim()
    );
    Ok(())
}

fn list_sources(runtime: &RuntimeContext, path: Option<&Path>) -> Result<()> {
    let manifest_root = resolve_manifest_root(path)?;
    let manifest = require_manifest(&manifest_root)?;
    if manifest.nuget_source_definitions().is_empty() {
        println!(
            "no source-backed NuGet packages are declared in {}",
            manifest_path(&manifest_root).display()
        );
        return Ok(());
    }

    println!(
        "Source-backed NuGet packages in {}:",
        manifest_path(&manifest_root).display()
    );
    for (name, source) in manifest.nuget_source_definitions() {
        println!(
            "- {} | repo {} | package {} | version {}",
            name,
            source.repo,
            source.package_id(name),
            source.version.as_deref().unwrap_or("(project/default)")
        );
        if let Some(state) = nuget::load_source_state(runtime, name)? {
            println!(
                "  last build: {} @ {}",
                state.version,
                state.commit.chars().take(7).collect::<String>()
            );
        }
    }
    Ok(())
}

fn build_source(
    runtime: &RuntimeContext,
    name: &str,
    path: Option<&Path>,
    force: bool,
) -> Result<()> {
    let manifest_root = resolve_manifest_root(path)?;
    let result = nuget::build_source_package(runtime, &manifest_root, name, force)?;
    println!("repo: {}", result.repo.key);
    println!("repo dir: {}", result.repo_dir.display());
    println!("package: {}", result.package.package_id);
    println!("version: {}", result.package.version);
    println!("commit: {}", result.state.commit);
    println!("output: {}", result.package.path.display());
    println!(
        "{}",
        if result.rebuilt {
            "source package rebuilt"
        } else {
            "source package already current"
        }
    );
    Ok(())
}

fn build_all_sources(runtime: &RuntimeContext, path: Option<&Path>, force: bool) -> Result<()> {
    let manifest_root = resolve_manifest_root(path)?;
    let results = nuget::build_all_sources(runtime, &manifest_root, force)?;
    if results.is_empty() {
        println!(
            "no source-backed NuGet packages are declared in {}",
            manifest_path(&manifest_root).display()
        );
        return Ok(());
    }
    for result in results {
        println!(
            "- {} {} -> {}",
            result.package.package_id,
            result.package.version,
            result.package.path.display()
        );
    }
    Ok(())
}

fn update_source(runtime: &RuntimeContext, name: &str, path: Option<&Path>) -> Result<()> {
    let manifest_root = resolve_manifest_root(path)?;
    let manifest = require_manifest(&manifest_root)?;
    let source = manifest
        .nuget_source_definitions()
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("nuget source '{}' was not found in mntpack.json", name))?;
    let sync = nuget::sync_source_repo(runtime, name, source)?;
    println!("repo: {}", sync.repo.key);
    println!("repo dir: {}", sync.repo_dir.display());
    println!("commit: {}", sync.commit);
    Ok(())
}

fn sync_sources(runtime: &RuntimeContext, path: Option<&Path>, force: bool) -> Result<()> {
    let manifest_root = resolve_manifest_root(path)?;
    let results = nuget::sync_all_sources(runtime, &manifest_root, force)?;
    if results.is_empty() {
        println!(
            "no source-backed NuGet packages are declared in {}",
            manifest_path(&manifest_root).display()
        );
        return Ok(());
    }
    for result in results {
        println!(
            "- {} {} -> {}",
            result.package.package_id,
            result.package.version,
            result.package.path.display()
        );
    }
    Ok(())
}

fn resolve_requested_package(
    runtime: &RuntimeContext,
    manifest_root: &Path,
    package_id: &str,
    version: Option<&str>,
    source: Option<&str>,
) -> Result<NugetPackage> {
    let manifest = Manifest::load(manifest_root)?;
    let existing = manifest
        .as_ref()
        .map(Manifest::resolved_nuget_packages)
        .unwrap_or_default()
        .into_iter()
        .find(|package| package.id.eq_ignore_ascii_case(package_id));
    let source_definition = manifest
        .as_ref()
        .and_then(|manifest| nuget::find_source_for_package(manifest, package_id));

    let mut package = NugetPackage {
        id: package_id.to_string(),
        version: clean_str(version)
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|package| package.version.clone())
            })
            .or_else(|| {
                source_definition
                    .as_ref()
                    .and_then(|(_, source)| source.version.clone())
            }),
        source: clean_str(source)
            .or_else(|| existing.as_ref().and_then(|package| package.source.clone()))
            .or_else(|| {
                source_definition
                    .as_ref()
                    .map(|_| nuget::MNTPACK_LOCAL_SOURCE_KEY.to_string())
            }),
    };

    if package
        .source
        .as_deref()
        .map(|value| value.eq_ignore_ascii_case(nuget::MNTPACK_LOCAL_SOURCE_KEY))
        .unwrap_or(false)
    {
        if let Some(feed_package) = nuget::ensure_source_package_available(
            runtime,
            manifest_root,
            package_id,
            package.version.as_deref(),
        )? {
            if package.version.is_none() {
                package.version = Some(feed_package.version);
            }
        } else if package.version.is_none() {
            if let Some(feed_package) = nuget::find_feed_package(runtime, package_id, None)? {
                package.version = Some(feed_package.version);
            }
        }
    }

    Ok(package)
}

fn resolve_manifest_root(path: Option<&Path>) -> Result<PathBuf> {
    let explicit = resolve_base_path(path)?;
    for ancestor in explicit.ancestors() {
        if manifest_path(ancestor).exists() {
            return Ok(ancestor.to_path_buf());
        }
    }
    Ok(explicit)
}

fn resolve_consumer_root(path: Option<&Path>, project: Option<&Path>) -> Result<PathBuf> {
    let explicit = resolve_base_path(path)?;
    for ancestor in explicit.ancestors() {
        if manifest_path(ancestor).exists() {
            return Ok(ancestor.to_path_buf());
        }
    }
    if let Ok(target) = dotnet::resolve_target(&explicit, project, false) {
        return Ok(target.workspace_root);
    }
    Ok(explicit)
}

fn resolve_base_path(path: Option<&Path>) -> Result<PathBuf> {
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

fn clean_str(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn clean_path(value: Option<&Path>) -> Option<String> {
    value.map(|path| path.to_string_lossy().replace('\\', "/"))
}

fn refresh_package_cache(package: &NugetPackage) -> Result<()> {
    if !package
        .source
        .as_deref()
        .map(|value| value.eq_ignore_ascii_case(nuget::MNTPACK_LOCAL_SOURCE_KEY))
        .unwrap_or(false)
    {
        return Ok(());
    }

    let removed = nuget::clear_global_package_cache(&package.id, package.version.as_deref())?;
    for path in removed {
        println!("refreshed cache entry {}", path.display());
    }
    Ok(())
}

fn refresh_packages_cache(packages: &[NugetPackage]) -> Result<()> {
    for package in packages {
        refresh_package_cache(package)?;
    }
    Ok(())
}
