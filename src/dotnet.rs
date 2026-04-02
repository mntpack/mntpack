use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use walkdir::{DirEntry, WalkDir};
use xmltree::{Element, EmitterConfig, XMLNode};

use crate::{
    config::{RuntimeContext, normalize_path_for_os},
    package::manifest::{Manifest, NugetPackage},
};

pub const NUGET_CONFIG_FILE: &str = "NuGet.config";
pub const MNTPACK_LOCAL_SOURCE_KEY: &str = "mntpack-local";

#[derive(Debug, Clone, Default)]
pub struct DotnetDiscovery {
    pub solutions: Vec<PathBuf>,
    pub projects: Vec<PathBuf>,
    pub has_directory_build_props: bool,
    pub has_directory_build_targets: bool,
    pub has_global_json: bool,
}

impl DotnetDiscovery {
    pub fn is_dotnet(&self) -> bool {
        !self.solutions.is_empty()
            || !self.projects.is_empty()
            || self.has_directory_build_props
            || self.has_directory_build_targets
            || self.has_global_json
    }
}

#[derive(Debug, Clone)]
pub struct DotnetTarget {
    pub workspace_root: PathBuf,
    pub solution: Option<PathBuf>,
    pub project: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct NugetConfigUpdate {
    pub path: PathBuf,
    pub source_key: String,
    pub source_value: String,
    pub changed: bool,
}

pub fn discover(root: &Path) -> Result<DotnetDiscovery> {
    let mut discovery = DotnetDiscovery::default();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(should_visit)
    {
        let entry = entry.with_context(|| format!("failed to walk {}", root.display()))?;
        if !entry.file_type().is_file() {
            continue;
        }

        let file_name = entry.file_name().to_string_lossy();
        let path = entry.path().to_path_buf();

        if has_extension(entry.path(), "sln") || has_extension(entry.path(), "slnx") {
            discovery.solutions.push(path);
            continue;
        }
        if has_extension(entry.path(), "csproj") {
            discovery.projects.push(path);
            continue;
        }
        if file_name.eq_ignore_ascii_case("Directory.Build.props") {
            discovery.has_directory_build_props = true;
            continue;
        }
        if file_name.eq_ignore_ascii_case("Directory.Build.targets") {
            discovery.has_directory_build_targets = true;
            continue;
        }
        if file_name.eq_ignore_ascii_case("global.json") {
            discovery.has_global_json = true;
        }
    }

    discovery.solutions.sort();
    discovery.projects.sort();
    Ok(discovery)
}

pub fn is_dotnet_project(root: &Path) -> bool {
    discover(root)
        .map(|value| value.is_dotnet())
        .unwrap_or(false)
}

pub fn build_hint(root: &Path, dotnet: &str) -> Result<Option<String>> {
    let target = resolve_build_target(root)?;
    Ok(target.map(|target| {
        format!(
            "{} build {} --configuration Release",
            dotnet,
            display_path(&target)
        )
    }))
}

pub fn ensure_local_feed(runtime: &RuntimeContext) -> Result<PathBuf> {
    fs::create_dir_all(&runtime.paths.nuget_source).with_context(|| {
        format!(
            "failed to create local NuGet source {}",
            runtime.paths.nuget_source.display()
        )
    })?;
    Ok(runtime.paths.nuget_source.clone())
}

pub fn ensure_workspace_config(
    runtime: &RuntimeContext,
    root: &Path,
    project_hint: Option<&Path>,
) -> Result<NugetConfigUpdate> {
    let target = resolve_target(root, project_hint, false)?;
    let feed_path = ensure_local_feed(runtime)?;
    ensure_nuget_config(&target.workspace_root, &feed_path)
}

pub fn resolve_build_target(root: &Path) -> Result<Option<PathBuf>> {
    let discovery = discover(root)?;
    if let Some(solution) = discovery.solutions.first() {
        return Ok(Some(solution.clone()));
    }
    if let Some(project) = discovery.projects.first() {
        return Ok(Some(project.clone()));
    }
    Ok(None)
}

pub fn resolve_target(
    root: &Path,
    project_hint: Option<&Path>,
    require_project: bool,
) -> Result<DotnetTarget> {
    let discovery = discover(root)?;
    if !discovery.is_dotnet() {
        bail!(
            "no .NET solution or project files were detected in {}",
            root.display()
        );
    }

    let solution = discovery.solutions.first().cloned();
    let project = select_project(root, &discovery, project_hint, require_project)?;
    let workspace_root = if let Some(solution) = solution.as_ref() {
        solution
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.to_path_buf())
    } else if let Some(project) = project.as_ref() {
        project
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.to_path_buf())
    } else {
        root.to_path_buf()
    };

    Ok(DotnetTarget {
        workspace_root,
        solution,
        project,
    })
}

pub fn build(runtime: &RuntimeContext, root: &Path) -> Result<Option<PathBuf>> {
    let Some(target) = resolve_build_target(root)? else {
        return Ok(None);
    };

    run_dotnet_command(
        runtime,
        root,
        &[
            "build".to_string(),
            display_path(&target),
            "--configuration".to_string(),
            "Release".to_string(),
        ],
    )?;

    Ok(Some(target))
}

pub fn restore(runtime: &RuntimeContext, root: &Path, project_hint: Option<&Path>) -> Result<()> {
    let target = resolve_target(root, project_hint, false)?;
    let restore_target = target.solution.or(target.project).ok_or_else(|| {
        anyhow::anyhow!(
            "no .NET solution or project file was found to restore in {}",
            root.display()
        )
    })?;
    run_dotnet_command(
        runtime,
        root,
        &["restore".to_string(), display_path(&restore_target)],
    )
}

pub fn add_package_reference(
    runtime: &RuntimeContext,
    root: &Path,
    project_hint: Option<&Path>,
    package: &NugetPackage,
) -> Result<PathBuf> {
    let target = resolve_target(root, project_hint, true)?;
    let project = target.project.as_ref().expect("project required");
    let mut args = vec![
        "add".to_string(),
        display_path(project),
        "package".to_string(),
        package.id.clone(),
        "--no-restore".to_string(),
    ];
    if let Some(version) = package.version.as_deref().filter(|value| !value.is_empty()) {
        args.push("--version".to_string());
        args.push(version.to_string());
    }
    if let Some(source) = package.source_value(runtime) {
        args.push("--source".to_string());
        args.push(source);
    }

    run_dotnet_command(runtime, root, &args)?;
    Ok(project.clone())
}

pub fn remove_package_reference(
    runtime: &RuntimeContext,
    root: &Path,
    project_hint: Option<&Path>,
    package_id: &str,
) -> Result<PathBuf> {
    let target = resolve_target(root, project_hint, true)?;
    let project = target.project.as_ref().expect("project required");
    run_dotnet_command(
        runtime,
        root,
        &[
            "remove".to_string(),
            display_path(project),
            "package".to_string(),
            package_id.to_string(),
        ],
    )?;
    Ok(project.clone())
}

pub fn apply_manifest_packages(
    runtime: &RuntimeContext,
    root: &Path,
    project_hint: Option<&Path>,
    manifest: &Manifest,
) -> Result<Vec<NugetPackage>> {
    let packages = manifest.resolved_nuget_packages();
    if packages.is_empty() {
        return Ok(Vec::new());
    }

    for package in &packages {
        add_package_reference(runtime, root, project_hint, package)?;
    }

    restore(runtime, root, project_hint)?;
    Ok(packages)
}

pub fn ensure_nuget_config(workspace_root: &Path, feed_path: &Path) -> Result<NugetConfigUpdate> {
    fs::create_dir_all(workspace_root)
        .with_context(|| format!("failed to create {}", workspace_root.display()))?;

    let config_path = workspace_root.join(NUGET_CONFIG_FILE);
    let mut root = if config_path.exists() {
        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read {}", config_path.display()))?;
        Element::parse(content.as_bytes())
            .with_context(|| format!("failed to parse {}", config_path.display()))?
    } else {
        Element::new("configuration")
    };

    if root.name != "configuration" {
        bail!(
            "{} exists but does not have a <configuration> root element",
            config_path.display()
        );
    }

    let source_value = normalize_path_for_os(feed_path);
    let package_sources = get_or_insert_child(&mut root, "packageSources");
    let mut changed = false;
    let mut found_key = false;
    let mut found_matching_value = false;

    for child in &mut package_sources.children {
        let XMLNode::Element(element) = child else {
            continue;
        };
        if element.name != "add" {
            continue;
        }

        let key = element.attributes.get("key").cloned().unwrap_or_default();
        let value = element.attributes.get("value").cloned().unwrap_or_default();
        if key == MNTPACK_LOCAL_SOURCE_KEY {
            found_key = true;
            if value != source_value {
                element
                    .attributes
                    .insert("value".to_string(), source_value.clone());
                changed = true;
            }
        }
        if normalize_source_value(&value) == normalize_source_value(&source_value) {
            found_matching_value = true;
        }
    }

    if !found_key && !found_matching_value {
        let mut add = Element::new("add");
        add.attributes
            .insert("key".to_string(), MNTPACK_LOCAL_SOURCE_KEY.to_string());
        add.attributes
            .insert("value".to_string(), source_value.clone());
        package_sources.children.push(XMLNode::Element(add));
        changed = true;
    }

    if changed || !config_path.exists() {
        let mut buffer = Vec::new();
        root.write_with_config(
            &mut buffer,
            EmitterConfig::new()
                .perform_indent(true)
                .write_document_declaration(true),
        )
        .with_context(|| format!("failed to write {}", config_path.display()))?;
        fs::write(&config_path, buffer)
            .with_context(|| format!("failed to write {}", config_path.display()))?;
    }

    Ok(NugetConfigUpdate {
        path: config_path,
        source_key: MNTPACK_LOCAL_SOURCE_KEY.to_string(),
        source_value,
        changed,
    })
}

fn select_project(
    root: &Path,
    discovery: &DotnetDiscovery,
    project_hint: Option<&Path>,
    require_project: bool,
) -> Result<Option<PathBuf>> {
    if let Some(project_hint) = project_hint {
        let candidate = if project_hint.is_absolute() {
            project_hint.to_path_buf()
        } else {
            root.join(project_hint)
        };
        if !candidate.exists() {
            bail!("project file not found at {}", candidate.display());
        }
        if !has_extension(&candidate, "csproj") {
            bail!("project path must point to a .csproj file");
        }
        return Ok(Some(candidate));
    }

    match discovery.projects.len() {
        0 if require_project => bail!(
            "no .csproj file was detected under {}. pass --project if the project lives elsewhere",
            root.display()
        ),
        0 => Ok(None),
        1 => Ok(discovery.projects.first().cloned()),
        _ if require_project => bail!(
            "multiple .csproj files were detected under {}. pass --project to choose one",
            root.display()
        ),
        _ => Ok(discovery.projects.first().cloned()),
    }
}

fn run_dotnet_command(runtime: &RuntimeContext, cwd: &Path, args: &[String]) -> Result<()> {
    let status = Command::new(&runtime.config.paths.dotnet)
        .args(args)
        .current_dir(cwd)
        .status()
        .with_context(|| {
            format!(
                "failed to run '{}' in {}",
                format_command(&runtime.config.paths.dotnet, args),
                cwd.display()
            )
        })?;

    if !status.success() {
        bail!(
            "command '{}' failed with exit code {:?}",
            format_command(&runtime.config.paths.dotnet, args),
            status.code()
        );
    }

    Ok(())
}

fn get_or_insert_child<'a>(parent: &'a mut Element, name: &str) -> &'a mut Element {
    if let Some(index) = parent
        .children
        .iter()
        .position(|child| matches!(child, XMLNode::Element(element) if element.name == name))
    {
        return match parent.children.get_mut(index) {
            Some(XMLNode::Element(element)) => element,
            _ => unreachable!("child index must reference an element"),
        };
    }

    parent.children.push(XMLNode::Element(Element::new(name)));
    match parent.children.last_mut() {
        Some(XMLNode::Element(element)) => element,
        _ => unreachable!("last child must be the inserted element"),
    }
}

fn normalize_source_value(value: &str) -> String {
    value.trim().replace('\\', "/").to_ascii_lowercase()
}

fn format_command(program: &str, args: &[String]) -> String {
    if args.is_empty() {
        return program.to_string();
    }
    format!("{program} {}", args.join(" "))
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn should_visit(entry: &DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return true;
    }

    let name = entry.file_name().to_string_lossy();
    !matches!(
        name.as_ref(),
        ".git" | ".hg" | ".svn" | "node_modules" | "target" | "bin" | "obj"
    )
}

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|value| value.eq_ignore_ascii_case(extension))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{MNTPACK_LOCAL_SOURCE_KEY, discover, ensure_nuget_config};

    #[test]
    fn discovery_detects_dotnet_indicators() {
        let temp = tempdir().expect("tempdir");
        fs::write(temp.path().join("Sample.slnx"), "").expect("solution");
        fs::create_dir_all(temp.path().join("src")).expect("src dir");
        fs::write(temp.path().join("src").join("Tool.csproj"), "").expect("project");
        fs::write(temp.path().join("Directory.Build.props"), "").expect("props");

        let discovery = discover(temp.path()).expect("discover");
        assert!(discovery.is_dotnet());
        assert_eq!(discovery.solutions.len(), 1);
        assert_eq!(discovery.projects.len(), 1);
        assert!(discovery.has_directory_build_props);
    }

    #[test]
    fn ensure_nuget_config_merges_existing_sources() {
        let temp = tempdir().expect("tempdir");
        let config_path = temp.path().join("NuGet.config");
        fs::write(
            &config_path,
            r#"<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <packageSources>
    <add key="nuget.org" value="https://api.nuget.org/v3/index.json" />
  </packageSources>
</configuration>"#,
        )
        .expect("seed config");

        let feed = temp.path().join(".mntpack").join("nuget").join("source");
        let update = ensure_nuget_config(temp.path(), &feed).expect("update config");
        let content = fs::read_to_string(config_path).expect("read config");

        assert!(update.changed);
        assert_eq!(update.source_key, MNTPACK_LOCAL_SOURCE_KEY);
        assert!(content.contains("nuget.org"));
        assert!(content.contains(MNTPACK_LOCAL_SOURCE_KEY));
    }
}
