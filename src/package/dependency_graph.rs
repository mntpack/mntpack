use std::collections::{BTreeSet, HashMap};

use anyhow::Result;

use crate::{
    config::RuntimeContext,
    package::{
        manifest::Manifest,
        record::{PackageRecord, load_all_records},
        resolver::resolve_repo,
    },
};

#[derive(Debug, Clone, Default)]
pub struct DependencyGraph {
    parents: HashMap<String, BTreeSet<String>>,
}

impl DependencyGraph {
    pub fn parents_of(&self, package: &str) -> Vec<String> {
        self.parents
            .get(package)
            .map(|value| value.iter().cloned().collect())
            .unwrap_or_default()
    }
}

pub fn build(runtime: &RuntimeContext) -> Result<(DependencyGraph, Vec<PackageRecord>)> {
    let records = load_all_records(&runtime.paths.packages)?;
    let mut parents: HashMap<String, BTreeSet<String>> = HashMap::new();

    for record in &records {
        let repo_dir = runtime
            .paths
            .repo_dir_existing_or_new(&record.owner, &record.repo);
        let manifest = Manifest::load(&repo_dir)?;
        let Some(manifest) = manifest else {
            continue;
        };
        for dep_spec in &manifest.dependencies {
            let matches = resolve_dependency_to_packages(runtime, dep_spec, &records);
            for dep in matches {
                parents
                    .entry(dep)
                    .or_default()
                    .insert(record.package_name.clone());
            }
        }
    }

    Ok((DependencyGraph { parents }, records))
}

fn resolve_dependency_to_packages(
    runtime: &RuntimeContext,
    dependency: &str,
    records: &[PackageRecord],
) -> Vec<String> {
    let dep = dependency.trim();
    if dep.is_empty() {
        return Vec::new();
    }

    if records.iter().any(|record| record.package_name == dep) {
        return vec![dep.to_string()];
    }

    if let Ok(resolved) = resolve_repo(dep, &runtime.config.default_owner) {
        return records
            .iter()
            .filter(|record| record.owner == resolved.owner && record.repo == resolved.repo)
            .map(|record| record.package_name.clone())
            .collect();
    }

    Vec::new()
}
