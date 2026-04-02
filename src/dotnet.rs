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
    nuget::MNTPACK_LOCAL_SOURCE_KEY,
    package::manifest::{Manifest, NugetPackage, NugetSourceDefinition},
};

pub const NUGET_CONFIG_FILE: &str = "NuGet.config";

#[derive(Debug, Clone, Default)]
pub struct DotnetDiscovery {
    pub solutions: Vec<PathBuf>,
    pub projects: Vec<PathBuf>,
    pub has_directory_build_props: bool,
    pub has_directory_build_targets: bool,
    pub has_global_json: bool,
}

#[derive(Debug, Clone)]
pub struct DotnetTarget {
    pub search_root: PathBuf,
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

#[derive(Debug, Clone)]
pub struct ProjectMetadata {
    pub package_id: String,
    pub version: Option<String>,
    pub is_packable: bool,
}

#[derive(Debug, Clone)]
pub struct SourceProjectResolution {
    pub working_root: PathBuf,
    pub solution: Option<PathBuf>,
    pub project: PathBuf,
    pub metadata: ProjectMetadata,
}

#[derive(Debug, Clone)]
pub struct PackedPackageExpectation {
    pub package_id: String,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct ProjectPackageReference {
    pub name: String,
    pub version: Option<String>,
}

pub trait DotnetRunner {
    fn run(&self, cwd: &Path, args: &[String]) -> Result<()>;
}

pub struct SystemDotnetRunner<'a> {
    runtime: &'a RuntimeContext,
}

impl<'a> SystemDotnetRunner<'a> {
    pub fn new(runtime: &'a RuntimeContext) -> Self {
        Self { runtime }
    }
}

impl DotnetRunner for SystemDotnetRunner<'_> {
    fn run(&self, cwd: &Path, args: &[String]) -> Result<()> {
        run_dotnet_command(self.runtime, cwd, args)
    }
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

pub fn discover(root: &Path) -> Result<DotnetDiscovery> {
    let search_root = resolve_search_root(root);
    discover_under(&search_root)
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
    fs::create_dir_all(&runtime.paths.nuget_feed).with_context(|| {
        format!(
            "failed to create local NuGet feed {}",
            runtime.paths.nuget_feed.display()
        )
    })?;
    Ok(runtime.paths.nuget_feed.clone())
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
    let target = resolve_target(root, None, false)?;
    Ok(target.solution.or(target.project))
}

pub fn resolve_target(
    root: &Path,
    project_hint: Option<&Path>,
    require_project: bool,
) -> Result<DotnetTarget> {
    let search_root = resolve_search_root(root);
    let discovery = discover_under(&search_root)?;
    if !discovery.is_dotnet() {
        bail!(
            "no .NET solution or project files were detected in {}",
            search_root.display()
        );
    }

    let solution = pick_solution(&search_root, &discovery, None)?;
    let project = select_project(&search_root, &discovery, project_hint, require_project)?;
    let workspace_root = if let Some(solution) = solution.as_ref() {
        solution
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| search_root.clone())
    } else if let Some(project) = project.as_ref() {
        project
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| search_root.clone())
    } else {
        search_root.clone()
    };

    Ok(DotnetTarget {
        search_root,
        workspace_root,
        solution,
        project,
    })
}

pub fn build(runtime: &RuntimeContext, root: &Path) -> Result<Option<PathBuf>> {
    let Some(target) = resolve_build_target(root)? else {
        return Ok(None);
    };
    let runner = SystemDotnetRunner::new(runtime);
    runner.run(
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
    let runner = SystemDotnetRunner::new(runtime);
    runner.run(
        &target.search_root,
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

    let runner = SystemDotnetRunner::new(runtime);
    runner.run(&target.search_root, &args)?;
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
    let runner = SystemDotnetRunner::new(runtime);
    runner.run(
        &target.search_root,
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

pub fn list_project_package_references(
    root: &Path,
    project_hint: Option<&Path>,
) -> Result<Vec<ProjectPackageReference>> {
    let target = resolve_target(root, project_hint, true)?;
    let project = target.project.as_ref().expect("project required");
    let document = read_xml_file(project)?;
    let mut packages = Vec::new();

    for child in &document.children {
        let XMLNode::Element(group) = child else {
            continue;
        };
        if group.name != "ItemGroup" {
            continue;
        }
        for item in &group.children {
            let XMLNode::Element(reference) = item else {
                continue;
            };
            if reference.name != "PackageReference" {
                continue;
            }
            let name = reference
                .attributes
                .get("Include")
                .or_else(|| reference.attributes.get("Update"))
                .cloned()
                .unwrap_or_default();
            if name.trim().is_empty() {
                continue;
            }
            let version = reference
                .attributes
                .get("Version")
                .cloned()
                .or_else(|| child_text(reference, "Version"));
            packages.push(ProjectPackageReference { name, version });
        }
    }

    packages.sort_by(|a, b| a.name.cmp(&b.name).then(a.version.cmp(&b.version)));
    Ok(packages)
}

pub fn resolve_source_project(
    repo_root: &Path,
    source_name: &str,
    source: &NugetSourceDefinition,
) -> Result<SourceProjectResolution> {
    let working_root = resolve_source_working_root(repo_root, source)?;
    let discovery = discover_under(&working_root)?;
    if !discovery.is_dotnet() {
        bail!(
            "nuget source '{}' does not contain a .NET project under {}",
            source_name,
            working_root.display()
        );
    }

    let solution = pick_solution(&working_root, &discovery, source.solution.as_deref())?;
    let project = pick_source_project(&working_root, &discovery, source.project.as_deref())?;
    let metadata = read_project_metadata(&project)?;

    if !metadata.is_packable {
        bail!(
            "project '{}' for nuget source '{}' is not packable",
            project.display(),
            source_name
        );
    }

    Ok(SourceProjectResolution {
        working_root,
        solution,
        project,
        metadata,
    })
}

pub fn expected_packed_package(
    _source_name: &str,
    source: &NugetSourceDefinition,
    resolution: &SourceProjectResolution,
) -> PackedPackageExpectation {
    let package_id = source
        .package_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| resolution.metadata.package_id.clone());
    let version = source
        .version
        .clone()
        .or_else(|| resolution.metadata.version.clone())
        .unwrap_or_else(|| "1.0.0".to_string());
    PackedPackageExpectation {
        package_id,
        version,
    }
}

pub fn pack_source_project(
    runtime: &RuntimeContext,
    resolution: &SourceProjectResolution,
    source_name: &str,
    source: &NugetSourceDefinition,
    feed_path: &Path,
) -> Result<PackedPackageExpectation> {
    let runner = SystemDotnetRunner::new(runtime);
    pack_source_project_with_runner(&runner, resolution, source_name, source, feed_path)
}

pub fn pack_source_project_with_runner(
    runner: &dyn DotnetRunner,
    resolution: &SourceProjectResolution,
    source_name: &str,
    source: &NugetSourceDefinition,
    feed_path: &Path,
) -> Result<PackedPackageExpectation> {
    fs::create_dir_all(feed_path)
        .with_context(|| format!("failed to create {}", feed_path.display()))?;

    let configuration = source.configuration().to_string();
    let expected = expected_packed_package(source_name, source, resolution);
    let mut common_props = Vec::new();
    if let Some(package_id) = source
        .package_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        common_props.push(format!("-p:PackageId={}", package_id.trim()));
    }
    if let Some(version) = source
        .version
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        common_props.push(format!("-p:PackageVersion={}", version.trim()));
        common_props.push(format!("-p:Version={}", version.trim()));
    }

    let restore_target = resolution.project.clone();
    let build_target = resolution.project.clone();
    let pack_target = resolution.project.clone();

    let restore_args = vec!["restore".to_string(), display_path(&restore_target)];
    runner.run(&resolution.working_root, &restore_args)?;

    let build_args = vec![
        "build".to_string(),
        display_path(&build_target),
        "--configuration".to_string(),
        configuration.clone(),
    ];
    runner.run(&resolution.working_root, &build_args)?;

    let mut pack_args = vec![
        "pack".to_string(),
        display_path(&pack_target),
        "--configuration".to_string(),
        configuration,
        "--output".to_string(),
        feed_path.to_string_lossy().to_string(),
        "--no-build".to_string(),
    ];
    pack_args.extend(common_props);
    runner.run(&resolution.working_root, &pack_args)?;

    Ok(expected)
}

pub fn read_project_metadata(project_path: &Path) -> Result<ProjectMetadata> {
    let document = read_xml_file(project_path)?;
    let fallback_name = project_path
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("Package")
        .to_string();

    let package_id = find_property(&document, "PackageId")
        .or_else(|| find_property(&document, "AssemblyName"))
        .unwrap_or_else(|| fallback_name.clone());
    let version =
        find_property(&document, "PackageVersion").or_else(|| find_property(&document, "Version"));
    let is_packable = find_property(&document, "IsPackable")
        .map(|value| !value.eq_ignore_ascii_case("false"))
        .unwrap_or(true);

    Ok(ProjectMetadata {
        package_id,
        version,
        is_packable,
    })
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

fn discover_under(root: &Path) -> Result<DotnetDiscovery> {
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

fn resolve_search_root(root: &Path) -> PathBuf {
    let base = if root.is_file() {
        root.parent().unwrap_or(root)
    } else {
        root
    };

    for ancestor in base.ancestors() {
        if has_direct_dotnet_indicators(ancestor) {
            return ancestor.to_path_buf();
        }
    }
    base.to_path_buf()
}

fn has_direct_dotnet_indicators(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().to_string();
        if has_extension(&path, "csproj")
            || has_extension(&path, "sln")
            || has_extension(&path, "slnx")
            || file_name.eq_ignore_ascii_case("Directory.Build.props")
            || file_name.eq_ignore_ascii_case("Directory.Build.targets")
            || file_name.eq_ignore_ascii_case("global.json")
        {
            return true;
        }
    }
    false
}

fn resolve_source_working_root(
    repo_root: &Path,
    source: &NugetSourceDefinition,
) -> Result<PathBuf> {
    let Some(subdir) = source
        .subdir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(repo_root.to_path_buf());
    };
    let path = resolve_explicit_path(repo_root, repo_root, subdir)?;
    if !path.is_dir() {
        bail!(
            "nuget source subdir '{}' is not a directory",
            path.display()
        );
    }
    Ok(path)
}

fn pick_solution(
    repo_root: &Path,
    discovery: &DotnetDiscovery,
    solution_hint: Option<&str>,
) -> Result<Option<PathBuf>> {
    if let Some(solution_hint) = solution_hint
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let candidate = resolve_explicit_path(repo_root, repo_root, solution_hint)?;
        if !candidate.exists() {
            bail!("solution file not found at {}", candidate.display());
        }
        if !(has_extension(&candidate, "sln") || has_extension(&candidate, "slnx")) {
            bail!("solution path must point to a .sln or .slnx file");
        }
        return Ok(Some(candidate));
    }

    match discovery.solutions.len() {
        0 => Ok(None),
        _ => Ok(discovery.solutions.first().cloned()),
    }
}

fn pick_source_project(
    repo_root: &Path,
    discovery: &DotnetDiscovery,
    project_hint: Option<&str>,
) -> Result<PathBuf> {
    if let Some(project_hint) = project_hint
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let candidate = resolve_explicit_path(repo_root, repo_root, project_hint)?;
        if !candidate.exists() {
            bail!("project file not found at {}", candidate.display());
        }
        if !has_extension(&candidate, "csproj") {
            bail!("project path must point to a .csproj file");
        }
        return Ok(candidate);
    }

    match discovery.projects.len() {
        0 => bail!("no .csproj file was detected under {}", repo_root.display()),
        1 => Ok(discovery.projects.first().cloned().expect("single project")),
        _ => bail!(
            "multiple .csproj files were detected under {}. set 'project' in mntpack.json for this nuget source",
            repo_root.display()
        ),
    }
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

fn resolve_explicit_path(base_root: &Path, working_root: &Path, value: &str) -> Result<PathBuf> {
    let candidate = PathBuf::from(value);
    if candidate.is_absolute() {
        return Ok(candidate);
    }
    let working_candidate = working_root.join(&candidate);
    if working_candidate.exists() {
        return Ok(working_candidate);
    }
    Ok(base_root.join(candidate))
}

fn read_xml_file(path: &Path) -> Result<Element> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    Element::parse(content.as_bytes())
        .with_context(|| format!("failed to parse {}", path.display()))
}

fn find_property(document: &Element, property_name: &str) -> Option<String> {
    for child in &document.children {
        let XMLNode::Element(group) = child else {
            continue;
        };
        if group.name != "PropertyGroup" {
            continue;
        }
        if let Some(value) = child_text(group, property_name) {
            return Some(value);
        }
    }
    None
}

fn child_text(element: &Element, name: &str) -> Option<String> {
    element
        .get_child(name)
        .and_then(Element::get_text)
        .map(|text| text.to_string())
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
    use std::{cell::RefCell, fs, path::Path};

    use anyhow::Result;
    use tempfile::tempdir;

    use super::{
        DotnetRunner, MNTPACK_LOCAL_SOURCE_KEY, NugetSourceDefinition, ProjectMetadata, discover,
        ensure_nuget_config, pack_source_project_with_runner, read_project_metadata,
        resolve_source_project, resolve_target,
    };

    struct RecordingRunner {
        calls: RefCell<Vec<Vec<String>>>,
    }

    impl RecordingRunner {
        fn new() -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl DotnetRunner for RecordingRunner {
        fn run(&self, _cwd: &Path, args: &[String]) -> Result<()> {
            self.calls.borrow_mut().push(args.to_vec());
            Ok(())
        }
    }

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

        let feed = temp.path().join(".mntpack").join("nuget").join("feed");
        let update = ensure_nuget_config(temp.path(), &feed).expect("update config");
        let content = fs::read_to_string(config_path).expect("read config");

        assert!(update.changed);
        assert_eq!(update.source_key, MNTPACK_LOCAL_SOURCE_KEY);
        assert!(content.contains("nuget.org"));
        assert!(content.contains(MNTPACK_LOCAL_SOURCE_KEY));
    }

    #[test]
    fn resolves_search_root_from_nested_directory() {
        let temp = tempdir().expect("tempdir");
        fs::create_dir_all(temp.path().join("src").join("Tool").join("Features")).expect("dirs");
        fs::write(temp.path().join("src").join("Tool").join("Tool.csproj"), "").expect("project");

        let target = resolve_target(
            &temp.path().join("src").join("Tool").join("Features"),
            None,
            true,
        )
        .expect("target");
        assert!(
            target
                .project
                .as_ref()
                .expect("project")
                .ends_with("Tool.csproj")
        );
    }

    #[test]
    fn reads_project_metadata_from_csproj() {
        let temp = tempdir().expect("tempdir");
        let project = temp.path().join("Tool.csproj");
        fs::write(
            &project,
            r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net10.0</TargetFramework>
    <PackageId>Fancy.Tool</PackageId>
    <Version>2.0.1</Version>
  </PropertyGroup>
</Project>"#,
        )
        .expect("project");

        let metadata = read_project_metadata(&project).expect("metadata");
        assert_eq!(metadata.package_id, "Fancy.Tool");
        assert_eq!(metadata.version.as_deref(), Some("2.0.1"));
    }

    #[test]
    fn source_project_resolution_requires_explicit_project_when_ambiguous() {
        let temp = tempdir().expect("tempdir");
        fs::write(temp.path().join("A.csproj"), "").expect("a");
        fs::write(temp.path().join("B.csproj"), "").expect("b");

        let err = resolve_source_project(
            temp.path(),
            "Test",
            &NugetSourceDefinition {
                repo: "owner/repo".to_string(),
                ..NugetSourceDefinition::default()
            },
        )
        .expect_err("expected ambiguity");
        assert!(err.to_string().contains("multiple .csproj"));
    }

    #[test]
    fn pack_orchestration_uses_restore_build_pack() {
        let temp = tempdir().expect("tempdir");
        let project = temp.path().join("Tool.csproj");
        fs::write(
            &project,
            r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net10.0</TargetFramework>
    <PackageId>Tool</PackageId>
  </PropertyGroup>
</Project>"#,
        )
        .expect("project");

        let resolution = super::SourceProjectResolution {
            working_root: temp.path().to_path_buf(),
            solution: None,
            project: project.clone(),
            metadata: ProjectMetadata {
                package_id: "Tool".to_string(),
                version: Some("1.0.0".to_string()),
                is_packable: true,
            },
        };
        let runner = RecordingRunner::new();
        let expectation = pack_source_project_with_runner(
            &runner,
            &resolution,
            "Tool",
            &NugetSourceDefinition {
                repo: "owner/repo".to_string(),
                version: Some("1.0.0-local.1".to_string()),
                ..NugetSourceDefinition::default()
            },
            &temp.path().join("feed"),
        )
        .expect("pack");

        assert_eq!(expectation.package_id, "Tool");
        assert_eq!(expectation.version, "1.0.0-local.1");
        let calls = runner.calls.borrow();
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0][0], "restore");
        assert_eq!(calls[1][0], "build");
        assert_eq!(calls[2][0], "pack");
    }
}
