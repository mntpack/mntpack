use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value, json};

const MANIFEST_FILE: &str = "mntpack.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReleaseAssetConfig {
    pub file: String,
    pub bin: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NugetPackageDeclaration {
    #[serde(rename = "name", alias = "id")]
    pub id: String,
    pub version: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum NugetPackageSpec {
    Simple(String),
    Detailed(NugetPackageDeclaration),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RunConfig {
    Single(String),
    PerTarget(HashMap<String, String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BinConfig {
    Path(String),
    Commands(HashMap<String, String>),
}

pub type NugetPackage = NugetPackageDeclaration;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NugetConfig {
    pub packages: Vec<NugetPackageSpec>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NugetSourceDefinition {
    #[serde(rename = "type", default = "default_nuget_source_type")]
    pub source_type: String,
    pub repo: String,
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    pub subdir: Option<String>,
    pub project: Option<String>,
    pub solution: Option<String>,
    pub package_id: Option<String>,
    pub version: Option<String>,
    pub configuration: Option<String>,
    pub output_mode: Option<String>,
    pub auto_build: Option<bool>,
    pub auto_update: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub name: Option<String>,
    pub version: Option<String>,
    pub preinstall: Option<String>,
    pub postinstall: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub nuget: NugetConfig,
    #[serde(default, rename = "nugetSources")]
    pub nuget_sources: BTreeMap<String, NugetSourceDefinition>,
    pub build: Option<String>,
    pub bin: Option<BinConfig>,
    pub run: Option<RunConfig>,
    #[serde(default)]
    pub release: HashMap<String, ReleaseAssetConfig>,
}

impl Manifest {
    pub fn load(repo_path: &Path) -> Result<Option<Self>> {
        let file = manifest_path(repo_path);
        if !file.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&file)
            .with_context(|| format!("failed to read {}", file.display()))?;
        let manifest = serde_json::from_str::<Self>(&content)
            .with_context(|| format!("failed to parse {}", file.display()))?;
        Ok(Some(manifest))
    }

    pub fn resolve_run_command(&self) -> Option<String> {
        let run = self.run.as_ref()?;
        match run {
            RunConfig::Single(command) => Some(command.clone()),
            RunConfig::PerTarget(targets) => targets.get(current_target()).cloned(),
        }
    }

    pub fn resolve_bin_path(&self) -> Option<String> {
        match self.bin.as_ref()? {
            BinConfig::Path(path) => Some(path.clone()),
            BinConfig::Commands(_) => None,
        }
    }

    pub fn resolve_bin_command(&self) -> Option<(String, String)> {
        match self.bin.as_ref()? {
            BinConfig::Path(_) => None,
            BinConfig::Commands(commands) => {
                let mut entries: Vec<(&String, &String)> = commands.iter().collect();
                entries.sort_by(|a, b| a.0.cmp(b.0));
                entries
                    .into_iter()
                    .next()
                    .map(|(name, command)| (name.clone(), command.clone()))
            }
        }
    }

    pub fn resolved_nuget_packages(&self) -> Vec<NugetPackage> {
        self.nuget
            .packages
            .iter()
            .filter_map(NugetPackage::from_spec)
            .collect()
    }

    pub fn nuget_source_definitions(&self) -> &BTreeMap<String, NugetSourceDefinition> {
        &self.nuget_sources
    }
}

impl NugetPackage {
    pub fn from_spec(spec: &NugetPackageSpec) -> Option<Self> {
        match spec {
            NugetPackageSpec::Simple(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    return None;
                }
                let (id, version) = match trimmed.rsplit_once('@') {
                    Some((id, version)) if !id.trim().is_empty() && !version.trim().is_empty() => {
                        (id.trim().to_string(), Some(version.trim().to_string()))
                    }
                    _ => (trimmed.to_string(), None),
                };
                Some(Self {
                    id,
                    version,
                    source: None,
                })
            }
            NugetPackageSpec::Detailed(value) => {
                let id = value.id.trim();
                if id.is_empty() {
                    return None;
                }
                Some(Self {
                    id: id.to_string(),
                    version: value
                        .version
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string),
                    source: value
                        .source
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string),
                })
            }
        }
    }

    pub fn source_value(&self, runtime: &crate::config::RuntimeContext) -> Option<String> {
        let source = self.source.as_deref()?.trim();
        if source.is_empty() {
            return None;
        }
        if source.eq_ignore_ascii_case(crate::nuget::MNTPACK_LOCAL_SOURCE_KEY) {
            return Some(runtime.paths.nuget_feed_value());
        }
        Some(source.to_string())
    }
}

impl NugetSourceDefinition {
    pub fn package_id<'a>(&'a self, source_name: &'a str) -> &'a str {
        self.package_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(source_name)
    }

    pub fn configuration(&self) -> &str {
        self.configuration
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Release")
    }

    pub fn output_mode(&self) -> &str {
        self.output_mode
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("feed")
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum NugetConfigRepr {
    Legacy(Vec<NugetPackageSpec>),
    Structured { packages: Vec<NugetPackageSpec> },
}

impl Serialize for NugetConfig {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = Map::new();
        state.insert(
            "packages".to_string(),
            serde_json::to_value(&self.packages).map_err(serde::ser::Error::custom)?,
        );
        Value::Object(state).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for NugetConfig {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let repr = Option::<NugetConfigRepr>::deserialize(deserializer)?;
        Ok(match repr {
            Some(NugetConfigRepr::Legacy(packages)) => Self { packages },
            Some(NugetConfigRepr::Structured { packages }) => Self { packages },
            None => Self::default(),
        })
    }
}

pub fn manifest_path(root: &Path) -> PathBuf {
    root.join(MANIFEST_FILE)
}

pub fn upsert_nuget_package(root: &Path, package: &NugetPackage) -> Result<bool> {
    let path = manifest_path(root);
    let mut document = read_manifest_document(&path)?;
    let object = document.as_object_mut().ok_or_else(|| {
        anyhow::anyhow!("{} must contain a JSON object at the root", path.display())
    })?;
    let packages_value = object
        .entry("nuget".to_string())
        .or_insert_with(|| json!({ "packages": [] }));
    let entries = get_or_insert_packages_array(packages_value, &path)?;

    let mut changed = false;
    let mut replaced = false;
    for entry in entries.iter_mut() {
        let Some(existing) = parse_value_as_nuget_package(entry) else {
            continue;
        };
        if existing.id.eq_ignore_ascii_case(&package.id) {
            let new_value = package_to_value(package);
            if *entry != new_value {
                *entry = new_value;
                changed = true;
            }
            replaced = true;
            break;
        }
    }

    if !replaced {
        entries.push(package_to_value(package));
        changed = true;
    }

    if changed || !path.exists() {
        write_manifest_document(&path, &document)?;
    }

    Ok(changed)
}

pub fn remove_nuget_package(root: &Path, package_id: &str) -> Result<bool> {
    let path = manifest_path(root);
    if !path.exists() {
        return Ok(false);
    }

    let mut document = read_manifest_document(&path)?;
    let object = document.as_object_mut().ok_or_else(|| {
        anyhow::anyhow!("{} must contain a JSON object at the root", path.display())
    })?;
    let Some(packages_value) = object.get_mut("nuget") else {
        return Ok(false);
    };
    let entries = get_or_insert_packages_array(packages_value, &path)?;

    let before = entries.len();
    entries.retain(|entry| {
        parse_value_as_nuget_package(entry)
            .map(|existing| !existing.id.eq_ignore_ascii_case(package_id))
            .unwrap_or(true)
    });

    if entries.len() == before {
        return Ok(false);
    }
    if entries.is_empty() {
        object.remove("nuget");
    }

    write_manifest_document(&path, &document)?;
    Ok(true)
}

pub fn upsert_nuget_source(
    root: &Path,
    source_name: &str,
    source: &NugetSourceDefinition,
) -> Result<bool> {
    let path = manifest_path(root);
    let mut document = read_manifest_document(&path)?;
    let object = document.as_object_mut().ok_or_else(|| {
        anyhow::anyhow!("{} must contain a JSON object at the root", path.display())
    })?;
    let sources = object
        .entry("nugetSources".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let source_map = sources
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("'nugetSources' in {} must be an object", path.display()))?;
    let new_value = serde_json::to_value(source)?;

    let changed = match source_map.get(source_name) {
        Some(existing) if *existing == new_value => false,
        _ => {
            source_map.insert(source_name.to_string(), new_value);
            true
        }
    };

    if changed || !path.exists() {
        write_manifest_document(&path, &document)?;
    }

    Ok(changed)
}

fn get_or_insert_packages_array<'a>(
    nuget_value: &'a mut Value,
    path: &Path,
) -> Result<&'a mut Vec<Value>> {
    if nuget_value.is_array() {
        let packages = nuget_value.take();
        *nuget_value = json!({ "packages": packages });
    }

    let packages = nuget_value
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("'nuget' in {} must be an object or array", path.display()))?
        .entry("packages".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    packages
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("'nuget.packages' in {} must be an array", path.display()))
}

fn package_to_value(package: &NugetPackage) -> Value {
    json!({
        "name": package.id,
        "version": package.version,
        "source": package.source,
    })
}

fn parse_value_as_nuget_package(value: &Value) -> Option<NugetPackage> {
    serde_json::from_value::<NugetPackageSpec>(value.clone())
        .ok()
        .and_then(|spec| NugetPackage::from_spec(&spec))
}

fn default_nuget_source_type() -> String {
    "github".to_string()
}

fn read_manifest_document(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(Value::Object(Map::new()));
    }

    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str::<Value>(&content)
        .with_context(|| format!("failed to parse {}", path.display()))
}

fn write_manifest_document(path: &Path, value: &Value) -> Result<()> {
    let serialized = serde_json::to_string_pretty(value)?;
    fs::write(path, format!("{serialized}\n"))
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn current_target() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "windows-x64",
        ("windows", "x86") => "windows-x86",
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-arm64",
        ("macos", "x86_64") => "macos-x64",
        ("macos", "aarch64") => "macos-arm64",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{
        Manifest, NugetConfig, NugetPackage, NugetPackageDeclaration, NugetPackageSpec,
        NugetSourceDefinition, remove_nuget_package, upsert_nuget_package, upsert_nuget_source,
    };

    #[test]
    fn resolves_nuget_packages_from_mixed_specs() {
        let manifest = Manifest {
            nuget: NugetConfig {
                packages: vec![
                    NugetPackageSpec::Simple("Newtonsoft.Json@13.0.3".to_string()),
                    NugetPackageSpec::Detailed(NugetPackageDeclaration {
                        id: "Serilog".to_string(),
                        version: Some("4.0.0".to_string()),
                        source: Some("mntpack-local".to_string()),
                    }),
                ],
            },
            ..Manifest::default()
        };

        let packages = manifest.resolved_nuget_packages();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].id, "Newtonsoft.Json");
        assert_eq!(packages[0].version.as_deref(), Some("13.0.3"));
        assert_eq!(packages[1].source.as_deref(), Some("mntpack-local"));
    }

    #[test]
    fn upsert_and_remove_nuget_packages_updates_manifest_file() {
        let temp = tempdir().expect("tempdir");
        let package = NugetPackage {
            id: "Newtonsoft.Json".to_string(),
            version: Some("13.0.3".to_string()),
            source: None,
        };

        assert!(upsert_nuget_package(temp.path(), &package).expect("upsert"));
        let content = fs::read_to_string(temp.path().join("mntpack.json")).expect("read");
        assert!(content.contains("Newtonsoft.Json"));
        assert!(content.contains("\"packages\""));

        assert!(remove_nuget_package(temp.path(), "Newtonsoft.Json").expect("remove"));
        let content = fs::read_to_string(temp.path().join("mntpack.json")).expect("read");
        assert!(!content.contains("Newtonsoft.Json"));
    }

    #[test]
    fn parses_legacy_array_shape_for_nuget_packages() {
        let manifest: Manifest = serde_json::from_str(
            r#"{
  "nuget": [
    { "id": "Newtonsoft.Json", "version": "13.0.3" }
  ]
}"#,
        )
        .expect("parse manifest");

        let packages = manifest.resolved_nuget_packages();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].id, "Newtonsoft.Json");
    }

    #[test]
    fn upsert_nuget_source_updates_manifest_file() {
        let temp = tempdir().expect("tempdir");
        let source = NugetSourceDefinition {
            repo: "owner/repo".to_string(),
            project: Some("src/Tool/Tool.csproj".to_string()),
            package_id: Some("Tool".to_string()),
            version: Some("1.0.0-local.1".to_string()),
            ..NugetSourceDefinition::default()
        };

        assert!(upsert_nuget_source(temp.path(), "Tool", &source).expect("upsert source"));
        let content = fs::read_to_string(temp.path().join("mntpack.json")).expect("read");
        assert!(content.contains("\"nugetSources\""));
        assert!(content.contains("\"Tool\""));
    }
}
