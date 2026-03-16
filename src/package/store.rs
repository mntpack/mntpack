use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)
        .with_context(|| format!("failed to open file for hashing {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn normalize_hash(hash: &str) -> String {
    hash.trim()
        .strip_prefix("sha256:")
        .unwrap_or(hash.trim())
        .to_ascii_lowercase()
}

pub fn hash_store_dir(store_root: &Path, hash: &str) -> PathBuf {
    store_root.join("sha256").join(normalize_hash(hash))
}

pub fn hash_store_entry(hash: &str) -> String {
    format!("sha256/{}", normalize_hash(hash))
}

pub fn sanitize_store_component(input: &str) -> String {
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

pub fn version_store_dir(store_root: &Path, repo_name: &str, version: &str) -> PathBuf {
    store_root
        .join("versions")
        .join(sanitize_store_component(repo_name))
        .join(sanitize_store_component(version))
}

pub fn executable_in_hash_store(
    store_root: &Path,
    hash: &str,
    preferred_name: Option<&str>,
) -> Result<Option<PathBuf>> {
    let store_dir = hash_store_dir(store_root, hash);
    if !store_dir.exists() {
        return Ok(None);
    }

    if let Some(name) = preferred_name {
        let candidate = store_dir.join(name);
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
    }

    let mut files = Vec::new();
    for entry in WalkDir::new(&store_dir).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        files.push(entry.path().to_path_buf());
    }
    files.sort();
    Ok(files.into_iter().next())
}

pub fn first_file_in_dir(dir: &Path) -> Option<PathBuf> {
    let mut files = Vec::new();
    for entry in WalkDir::new(dir).into_iter().flatten() {
        if entry.file_type().is_file() {
            files.push(entry.path().to_path_buf());
        }
    }
    files.sort();
    files.into_iter().next()
}

pub fn require_binary_name(path: &Path, fallback: &str) -> Result<String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
        .unwrap_or_else(|| fallback.to_string());
    if file_name.trim().is_empty() {
        bail!("unable to determine binary file name");
    }
    Ok(file_name)
}
