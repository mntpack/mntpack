use std::{
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use xmltree::Element;
use zip::ZipArchive;

use crate::{
    config::RuntimeContext,
    dotnet::{self, ProjectPackageReference},
    github::clone::{head_commit, sync_repo},
    package::{
        manifest::{Manifest, NugetSourceDefinition},
        resolver::{ResolvedRepo, resolve_repo},
        store::sanitize_store_component,
    },
};

pub const MNTPACK_LOCAL_SOURCE_KEY: &str = "mntpack-local";
const SOURCE_STATE_SUFFIX: &str = ".json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcePackageState {
    pub source_name: String,
    pub repo: String,
    pub repo_path: String,
    pub commit: String,
    pub package_id: String,
    pub version: String,
    pub project_path: String,
    pub solution_path: Option<String>,
    pub package_path: String,
    pub configuration: String,
    pub last_built_unix: u64,
}

#[derive(Debug, Clone)]
pub struct FeedPackageInfo {
    pub package_id: String,
    pub version: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct SourceBuildResult {
    pub package: FeedPackageInfo,
    pub state: SourcePackageState,
    pub repo: ResolvedRepo,
    pub repo_dir: PathBuf,
    pub rebuilt: bool,
}

#[derive(Debug, Clone)]
pub struct SourceSyncResult {
    pub repo: ResolvedRepo,
    pub repo_dir: PathBuf,
    pub commit: String,
}

pub fn ensure_feed(runtime: &RuntimeContext) -> Result<PathBuf> {
    fs::create_dir_all(&runtime.paths.nuget_feed).with_context(|| {
        format!(
            "failed to create NuGet feed directory {}",
            runtime.paths.nuget_feed.display()
        )
    })?;
    Ok(runtime.paths.nuget_feed.clone())
}

pub fn clear_global_package_cache(package_id: &str, version: Option<&str>) -> Result<Vec<PathBuf>> {
    let root = global_packages_root()?;
    clear_global_package_cache_under(&root, package_id, version)
}

fn clear_global_package_cache_under(
    root: &Path,
    package_id: &str,
    version: Option<&str>,
) -> Result<Vec<PathBuf>> {
    let package_id = package_id.trim();
    if package_id.is_empty() {
        bail!("package id cannot be empty");
    }

    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut removed = Vec::new();
    let package_dirs = matching_children(&root, package_id)?;
    for package_dir in package_dirs {
        if let Some(version) = version.map(str::trim).filter(|value| !value.is_empty()) {
            for version_dir in matching_children(&package_dir, version)? {
                remove_package_cache_path(&version_dir)?;
                removed.push(version_dir);
            }
            continue;
        }

        remove_package_cache_path(&package_dir)?;
        removed.push(package_dir);
    }

    removed.sort();
    Ok(removed)
}

pub fn list_feed_packages(runtime: &RuntimeContext) -> Result<Vec<FeedPackageInfo>> {
    ensure_feed(runtime)?;
    let mut packages = Vec::new();
    for entry in fs::read_dir(&runtime.paths.nuget_feed)
        .with_context(|| format!("failed to read {}", runtime.paths.nuget_feed.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        if !is_nupkg(&path) {
            continue;
        }
        if let Some(info) = read_nupkg_metadata(&path)? {
            packages.push(info);
        }
    }
    packages.sort_by(|a, b| {
        a.package_id
            .cmp(&b.package_id)
            .then(a.version.cmp(&b.version))
            .then(a.path.cmp(&b.path))
    });
    Ok(packages)
}

pub fn find_feed_package(
    runtime: &RuntimeContext,
    package_id: &str,
    version: Option<&str>,
) -> Result<Option<FeedPackageInfo>> {
    let mut matches: Vec<_> = list_feed_packages(runtime)?
        .into_iter()
        .filter(|package| package.package_id.eq_ignore_ascii_case(package_id))
        .filter(|package| {
            version
                .map(|expected| package.version.eq_ignore_ascii_case(expected))
                .unwrap_or(true)
        })
        .collect();
    matches.sort_by(|a, b| b.version.cmp(&a.version).then(a.path.cmp(&b.path)));
    Ok(matches.into_iter().next())
}

pub fn list_project_packages(
    root: &Path,
    project_hint: Option<&Path>,
) -> Result<Vec<ProjectPackageReference>> {
    dotnet::list_project_package_references(root, project_hint)
}

pub fn load_source_state(
    runtime: &RuntimeContext,
    source_name: &str,
) -> Result<Option<SourcePackageState>> {
    let path = source_state_path(runtime, source_name);
    if !path.exists() {
        return Ok(None);
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let state = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Some(state))
}

pub fn save_source_state(runtime: &RuntimeContext, state: &SourcePackageState) -> Result<()> {
    fs::create_dir_all(&runtime.paths.nuget_state)
        .with_context(|| format!("failed to create {}", runtime.paths.nuget_state.display()))?;
    let path = source_state_path(runtime, &state.source_name);
    let payload = serde_json::to_string_pretty(state)?;
    fs::write(&path, format!("{payload}\n"))
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn sync_source_repo(
    runtime: &RuntimeContext,
    source_name: &str,
    source: &NugetSourceDefinition,
) -> Result<SourceSyncResult> {
    if !source.source_type.trim().eq_ignore_ascii_case("github") {
        bail!(
            "unsupported nuget source type '{}' for '{}'; only 'github' is currently supported",
            source.source_type,
            source_name
        );
    }

    let resolved = resolve_repo(&source.repo, &runtime.config.default_owner)?;
    let repo_dir = runtime
        .paths
        .repo_dir_existing_or_new(&resolved.owner, &resolved.repo);
    sync_repo(
        &resolved,
        &repo_dir,
        &runtime.paths.cache_git,
        &runtime.config.paths.git,
        source.reference.as_deref(),
    )?;
    let commit = head_commit(&repo_dir)?;

    Ok(SourceSyncResult {
        repo: resolved,
        repo_dir,
        commit,
    })
}

pub fn build_source_package(
    runtime: &RuntimeContext,
    manifest_root: &Path,
    source_name: &str,
    force: bool,
) -> Result<SourceBuildResult> {
    let manifest = require_manifest(manifest_root)?;
    let source = manifest
        .nuget_source_definitions()
        .get(source_name)
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "nuget source '{}' was not found in mntpack.json",
                source_name
            )
        })?;

    validate_output_mode(source_name, &source)?;
    ensure_feed(runtime)?;
    let sync = sync_source_repo(runtime, source_name, &source)?;
    let resolution = dotnet::resolve_source_project(&sync.repo_dir, source_name, &source)?;
    let expectation = dotnet::expected_packed_package(source_name, &source, &resolution);

    if !force {
        if let Some(existing) = load_source_state(runtime, source_name)? {
            if existing.commit == sync.commit
                && existing
                    .package_id
                    .eq_ignore_ascii_case(&expectation.package_id)
                && existing.version.eq_ignore_ascii_case(&expectation.version)
                && PathBuf::from(&existing.package_path).exists()
            {
                return Ok(SourceBuildResult {
                    package: FeedPackageInfo {
                        package_id: existing.package_id.clone(),
                        version: existing.version.clone(),
                        path: PathBuf::from(&existing.package_path),
                    },
                    state: existing,
                    repo: sync.repo,
                    repo_dir: sync.repo_dir,
                    rebuilt: false,
                });
            }
        }
    }

    let expectation = dotnet::pack_source_project(
        runtime,
        &resolution,
        source_name,
        &source,
        &runtime.paths.nuget_feed,
    )?;
    let package = find_feed_package(runtime, &expectation.package_id, Some(&expectation.version))?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "package '{}' version '{}' was not found in {} after pack",
                expectation.package_id,
                expectation.version,
                runtime.paths.nuget_feed.display()
            )
        })?;

    let state = SourcePackageState {
        source_name: source_name.to_string(),
        repo: sync.repo.key.clone(),
        repo_path: sync.repo_dir.to_string_lossy().to_string(),
        commit: sync.commit,
        package_id: package.package_id.clone(),
        version: package.version.clone(),
        project_path: resolution.project.to_string_lossy().to_string(),
        solution_path: resolution
            .solution
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        package_path: package.path.to_string_lossy().to_string(),
        configuration: source.configuration().to_string(),
        last_built_unix: current_unix_time(),
    };
    save_source_state(runtime, &state)?;

    Ok(SourceBuildResult {
        package,
        state,
        repo: sync.repo,
        repo_dir: sync.repo_dir,
        rebuilt: true,
    })
}

pub fn build_all_sources(
    runtime: &RuntimeContext,
    manifest_root: &Path,
    force: bool,
) -> Result<Vec<SourceBuildResult>> {
    let manifest = require_manifest(manifest_root)?;
    let mut results = Vec::new();
    for source_name in manifest.nuget_source_definitions().keys() {
        results.push(build_source_package(
            runtime,
            manifest_root,
            source_name,
            force,
        )?);
    }
    Ok(results)
}

pub fn sync_all_sources(
    runtime: &RuntimeContext,
    manifest_root: &Path,
    force: bool,
) -> Result<Vec<SourceBuildResult>> {
    build_all_sources(runtime, manifest_root, force)
}

pub fn ensure_source_package_available(
    runtime: &RuntimeContext,
    manifest_root: &Path,
    package_id: &str,
    version: Option<&str>,
) -> Result<Option<FeedPackageInfo>> {
    if let Some(found) = find_feed_package(runtime, package_id, version)? {
        return Ok(Some(found));
    }

    let manifest = match Manifest::load(manifest_root)? {
        Some(manifest) => manifest,
        None => return Ok(None),
    };
    let Some((source_name, source)) = find_source_for_package(&manifest, package_id) else {
        return Ok(None);
    };

    let built = build_source_package(runtime, manifest_root, &source_name, false)?;
    if let Some(expected_version) = version {
        if !built.package.version.eq_ignore_ascii_case(expected_version) {
            bail!(
                "source '{}' built version '{}' but '{}' was requested",
                source_name,
                built.package.version,
                expected_version
            );
        }
    } else if let Some(source_version) = source.version.as_deref() {
        if !built.package.version.eq_ignore_ascii_case(source_version) {
            bail!(
                "source '{}' built version '{}' but config expected '{}'",
                source_name,
                built.package.version,
                source_version
            );
        }
    }

    Ok(Some(built.package))
}

pub fn find_source_for_package(
    manifest: &Manifest,
    package_id: &str,
) -> Option<(String, NugetSourceDefinition)> {
    manifest
        .nuget_source_definitions()
        .iter()
        .find(|(source_name, source)| {
            source_name.eq_ignore_ascii_case(package_id)
                || source
                    .package_id(source_name)
                    .eq_ignore_ascii_case(package_id)
        })
        .map(|(source_name, source)| (source_name.clone(), source.clone()))
}

pub fn source_state_path(runtime: &RuntimeContext, source_name: &str) -> PathBuf {
    runtime.paths.nuget_state.join(format!(
        "{}{}",
        sanitize_store_component(source_name),
        SOURCE_STATE_SUFFIX
    ))
}

fn require_manifest(root: &Path) -> Result<Manifest> {
    let Some(manifest) = Manifest::load(root)? else {
        bail!("mntpack.json was not found in {}", root.display());
    };
    Ok(manifest)
}

fn validate_output_mode(source_name: &str, source: &NugetSourceDefinition) -> Result<()> {
    match source.output_mode() {
        "feed" | "feed-and-cache" => Ok(()),
        other => bail!(
            "unsupported outputMode '{}' for nuget source '{}'",
            other,
            source_name
        ),
    }
}

fn is_nupkg(path: &Path) -> bool {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    file_name.ends_with(".nupkg") && !file_name.ends_with(".snupkg")
}

fn read_nupkg_metadata(path: &Path) -> Result<Option<FeedPackageInfo>> {
    let file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("failed to read NuGet archive {}", path.display()))?;

    let mut nuspec_index = None;
    for index in 0..archive.len() {
        let file = archive.by_index(index)?;
        if file.name().ends_with(".nuspec") {
            nuspec_index = Some(index);
            break;
        }
    }

    let Some(index) = nuspec_index else {
        return Ok(None);
    };

    let mut file = archive.by_index(index)?;
    let mut xml = String::new();
    file.read_to_string(&mut xml)
        .with_context(|| format!("failed to read nuspec from {}", path.display()))?;
    let root = Element::parse(xml.as_bytes())
        .with_context(|| format!("failed to parse nuspec from {}", path.display()))?;
    let metadata = root
        .get_child("metadata")
        .ok_or_else(|| anyhow::anyhow!("nuspec in {} is missing <metadata>", path.display()))?;
    let package_id = child_text(metadata, "id")
        .ok_or_else(|| anyhow::anyhow!("nuspec in {} is missing <id>", path.display()))?;
    let version = child_text(metadata, "version")
        .ok_or_else(|| anyhow::anyhow!("nuspec in {} is missing <version>", path.display()))?;

    Ok(Some(FeedPackageInfo {
        package_id,
        version,
        path: path.to_path_buf(),
    }))
}

fn child_text(element: &Element, name: &str) -> Option<String> {
    element
        .get_child(name)
        .and_then(Element::get_text)
        .map(|value| value.to_string())
}

fn current_unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn global_packages_root() -> Result<PathBuf> {
    if let Ok(custom) = env::var("NUGET_PACKAGES") {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    let home = dirs::home_dir().context("unable to locate user home directory")?;
    Ok(home.join(".nuget").join("packages"))
}

fn matching_children(root: &Path, expected: &str) -> Result<Vec<PathBuf>> {
    let mut matches = Vec::new();
    for entry in fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name.eq_ignore_ascii_case(expected) {
            matches.push(path);
        }
    }
    Ok(matches)
}

fn remove_package_cache_path(path: &Path) -> Result<()> {
    fs::remove_dir_all(path).with_context(|| format!("failed to remove {}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::io::Write;

    use tempfile::tempdir;
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    use crate::package::manifest::{Manifest, NugetSourceDefinition};

    use super::{
        clear_global_package_cache_under, find_source_for_package, read_nupkg_metadata,
        source_state_path,
    };

    #[test]
    fn finds_source_definition_by_package_id() {
        let manifest = Manifest {
            nuget_sources: BTreeMap::from([(
                "CS2Luau.Roblox".to_string(),
                NugetSourceDefinition {
                    repo: "owner/repo".to_string(),
                    package_id: Some("CS2Luau.Roblox".to_string()),
                    ..NugetSourceDefinition::default()
                },
            )]),
            ..Manifest::default()
        };

        let found = find_source_for_package(&manifest, "CS2Luau.Roblox");
        assert!(found.is_some());
    }

    #[test]
    fn reads_package_metadata_from_nupkg() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("Tool.1.0.0.nupkg");
        let file = std::fs::File::create(&path).expect("archive");
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        writer
            .start_file("Tool.nuspec", options)
            .expect("start file");
        write!(
            writer,
            r#"<?xml version="1.0" encoding="utf-8"?>
<package>
  <metadata>
    <id>Tool</id>
    <version>1.0.0</version>
  </metadata>
</package>"#
        )
        .expect("write nuspec");
        writer.finish().expect("finish");

        let info = read_nupkg_metadata(&path)
            .expect("metadata")
            .expect("metadata present");
        assert_eq!(info.package_id, "Tool");
        assert_eq!(info.version, "1.0.0");
    }

    #[test]
    fn builds_stable_source_state_path() {
        let runtime = crate::config::RuntimeContext::load_or_init;
        let _ = runtime;
        let temp = tempdir().expect("tempdir");
        let paths = crate::config::AppPaths {
            root: temp.path().to_path_buf(),
            config: temp.path().join("config.json"),
            repos: temp.path().join("repos"),
            packages: temp.path().join("packages"),
            cache: temp.path().join("cache"),
            cache_git: temp.path().join("cache").join("git"),
            cache_exec: temp.path().join("cache").join("exec"),
            nuget: temp.path().join("nuget"),
            nuget_feed: temp.path().join("nuget").join("feed"),
            nuget_state: temp.path().join("nuget").join("state"),
            store: temp.path().join("store"),
            bin: temp.path().join("bin"),
        };
        let runtime = crate::config::RuntimeContext {
            config: crate::config::Config::default(),
            paths,
        };

        let path = source_state_path(&runtime, "CS2Luau.Roblox");
        assert!(path.ends_with("CS2Luau.Roblox.json"));
    }

    #[test]
    fn clears_matching_global_package_cache_entries() {
        let temp = tempdir().expect("tempdir");
        let package_root = temp.path().join("nuget-cache");
        let version_dir = package_root.join("cs2luau.compiler").join("1.0.0-local.2");
        fs::create_dir_all(&version_dir).expect("version dir");

        let removed = clear_global_package_cache_under(
            &package_root,
            "CS2Luau.Compiler",
            Some("1.0.0-local.2"),
        )
        .expect("clear cache");

        assert_eq!(removed, vec![version_dir.clone()]);
        assert!(!version_dir.exists());
    }
}
