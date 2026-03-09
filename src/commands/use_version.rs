use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use walkdir::WalkDir;

use crate::{
    config::RuntimeContext,
    package::record::{find_record_by_package_name, save_record},
    shim::generator::create_shim,
};

pub fn execute(runtime: &RuntimeContext, package: &str, version: &str) -> Result<()> {
    let package_name = package.trim();
    let version_name = version.trim();
    if package_name.is_empty() {
        bail!("package name cannot be empty");
    }
    if version_name.is_empty() {
        bail!("version cannot be empty");
    }

    let package_dir = runtime.paths.package_dir(package_name);
    let Some(mut record) = find_record_by_package_name(&runtime.paths.packages, package_name)?
    else {
        bail!("package '{package_name}' is not installed");
    };

    let repo_segment = sanitize_store_component(&record.repo);
    let version_segment = sanitize_store_component(version_name);
    let store_dir = runtime
        .paths
        .store
        .join(&repo_segment)
        .join(&version_segment);
    if !store_dir.exists() {
        bail!(
            "version '{}' is not installed for package '{}' (missing {})",
            version_name,
            package_name,
            store_dir.display()
        );
    }

    let preferred_name = preferred_binary_name(&record);
    let binary = select_binary_from_store(&store_dir, preferred_name.as_deref())?;
    let shim_name = record
        .shim_name
        .clone()
        .unwrap_or_else(|| record.package_name.clone());

    record.version = Some(version_name.to_string());
    record.commit = None;
    record.store_entry = Some(format!("{repo_segment}/{version_segment}"));
    record.binary_path = Some(binary.to_string_lossy().to_string());
    record.binary_rel_path = Some(
        binary
            .strip_prefix(&runtime.paths.root)
            .unwrap_or(&binary)
            .to_string_lossy()
            .replace('\\', "/"),
    );
    record.build_pending = false;

    save_record(&package_dir, &record)?;

    if record.global {
        create_shim(runtime, &record.package_name, &shim_name, Some(&binary))?;
    }

    println!("using {} {}", record.package_name, version_name);
    Ok(())
}

fn preferred_binary_name(record: &crate::package::record::PackageRecord) -> Option<String> {
    if let Some(path) = record.binary_path.as_deref() {
        let path = Path::new(path);
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            return Some(name.to_string());
        }
    }
    if let Some(path) = record.binary_rel_path.as_deref() {
        let path = Path::new(path);
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            return Some(name.to_string());
        }
    }
    None
}

fn select_binary_from_store(store_dir: &Path, preferred_name: Option<&str>) -> Result<PathBuf> {
    if let Some(name) = preferred_name {
        let candidate = store_dir.join(name);
        if candidate.exists() && is_executable_candidate(&candidate)? {
            return Ok(candidate);
        }
    }

    let mut candidates = Vec::new();
    for entry in WalkDir::new(store_dir).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path().to_path_buf();
        if !is_executable_candidate(&path)? {
            continue;
        }
        candidates.push(path);
    }

    match candidates.len() {
        0 => bail!("no executable binary found in {}", store_dir.display()),
        1 => Ok(candidates.remove(0)),
        _ => {
            candidates.sort();
            Ok(candidates.remove(0))
        }
    }
}

fn is_executable_candidate(path: &Path) -> Result<bool> {
    if cfg!(windows) {
        let ext = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
        return Ok(ext.eq_ignore_ascii_case("exe"));
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

fn sanitize_store_component(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return "unknown".to_string();
    }
    trimmed
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect()
}
