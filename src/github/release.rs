use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::SystemTime,
};

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use serde::Deserialize;
use tar::Archive;
use walkdir::WalkDir;
use zip::ZipArchive;

use crate::{config::RuntimeContext, package::manifest::Manifest, package::resolver::ResolvedRepo};

#[derive(Debug, Deserialize)]
struct ReleaseResponse {
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

pub async fn try_download_release_binary(
    runtime: &RuntimeContext,
    resolved: &ResolvedRepo,
    manifest: &Manifest,
    version: Option<&str>,
    release_asset: Option<&str>,
) -> Result<Option<PathBuf>> {
    let target = current_target();
    let release_cfg = manifest.release.get(target);
    let asset_name = release_asset
        .map(|name| name.to_string())
        .or_else(|| release_cfg.map(|cfg| cfg.file.clone()));
    let Some(asset_name) = asset_name else {
        return Ok(None);
    };
    let expected_bin = release_cfg.map(|cfg| cfg.bin.clone());

    let client = reqwest::Client::builder()
        .user_agent("mntpack/0.1")
        .build()
        .context("failed to create http client")?;
    let api_url = if let Some(tag) = version {
        format!(
            "https://api.github.com/repos/{}/{}/releases/tags/{}",
            resolved.owner, resolved.repo, tag
        )
    } else {
        format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            resolved.owner, resolved.repo
        )
    };

    let response = client
        .get(&api_url)
        .send()
        .await
        .with_context(|| format!("failed to query github release api: {api_url}"))?;

    if response.status().as_u16() == 404 {
        return Ok(None);
    }

    let response = response
        .error_for_status()
        .with_context(|| format!("github api request failed for {}", resolved.key))?;
    let release = response
        .json::<ReleaseResponse>()
        .await
        .context("failed to parse github release response")?;

    let Some(asset) = release.assets.into_iter().find(|a| a.name == asset_name) else {
        return Ok(None);
    };

    let binary =
        download_and_extract_asset(runtime, resolved, &asset, expected_bin.as_deref()).await?;
    Ok(Some(binary))
}

async fn download_and_extract_asset(
    runtime: &RuntimeContext,
    resolved: &ResolvedRepo,
    asset: &ReleaseAsset,
    expected_bin: Option<&str>,
) -> Result<PathBuf> {
    let client = reqwest::Client::builder()
        .user_agent("mntpack/0.1")
        .build()
        .context("failed to create http client")?;
    let bytes = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .with_context(|| format!("failed to download release asset {}", asset.name))?
        .error_for_status()
        .with_context(|| format!("release asset download failed for {}", asset.name))?
        .bytes()
        .await
        .with_context(|| format!("failed to read downloaded bytes for {}", asset.name))?;

    let asset_path = runtime
        .paths
        .cache
        .join(format!("{}-{}", resolved.key, asset.name));
    tokio::fs::write(&asset_path, &bytes)
        .await
        .with_context(|| format!("failed to write {}", asset_path.display()))?;

    let extract_dir = runtime
        .paths
        .cache
        .join(format!("extract-{}", resolved.key));
    if extract_dir.exists() {
        fs::remove_dir_all(&extract_dir)
            .with_context(|| format!("failed to clean {}", extract_dir.display()))?;
    }
    fs::create_dir_all(&extract_dir)
        .with_context(|| format!("failed to create {}", extract_dir.display()))?;

    if asset.name.ends_with(".zip") {
        extract_zip(&asset_path, &extract_dir)?;
    } else if asset.name.ends_with(".tar.gz") || asset.name.ends_with(".tgz") {
        extract_tar_gz(&asset_path, &extract_dir)?;
    } else {
        let output = if let Some(relative_bin) = expected_bin {
            let direct_path = extract_dir.join(relative_bin);
            if let Some(parent) = direct_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            fs::copy(&asset_path, &direct_path).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    asset_path.display(),
                    direct_path.display()
                )
            })?;
            direct_path
        } else {
            let direct_path = extract_dir.join(&asset.name);
            fs::copy(&asset_path, &direct_path).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    asset_path.display(),
                    direct_path.display()
                )
            })?;
            direct_path
        };
        return Ok(output);
    }

    if let Some(relative_bin) = expected_bin {
        let configured = extract_dir.join(relative_bin);
        if configured.exists() {
            return Ok(configured);
        }

        let file_name = Path::new(relative_bin)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(relative_bin);

        for entry in WalkDir::new(&extract_dir).into_iter().flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            if entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(file_name)
            {
                return Ok(entry.path().to_path_buf());
            }
        }

        bail!(
            "release asset extracted but binary '{}' was not found",
            relative_bin
        )
    }

    let mut candidates: Vec<(PathBuf, SystemTime)> = Vec::new();
    for entry in WalkDir::new(&extract_dir).into_iter().flatten() {
        if entry.file_type().is_file() {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if cfg!(windows) {
                if !name.to_ascii_lowercase().ends_with(".exe") {
                    continue;
                }
            } else {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mode = fs::metadata(path)?.permissions().mode();
                    if mode & 0o111 == 0 {
                        continue;
                    }
                }
            }
            let modified = fs::metadata(path)
                .and_then(|meta| meta.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            candidates.push((path.to_path_buf(), modified));
        }
    }
    if candidates.is_empty() {
        bail!("release asset extracted but no executable binary was found");
    }
    if let Some((path, _)) = candidates.iter().find(|(path, _)| {
        path.file_stem()
            .and_then(|name| name.to_str())
            .map(|name| name.eq_ignore_ascii_case(&resolved.repo))
            .unwrap_or(false)
    }) {
        return Ok(path.clone());
    }
    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(candidates.remove(0).0)
}

fn extract_zip(zip_path: &Path, destination: &Path) -> Result<()> {
    let file =
        File::open(zip_path).with_context(|| format!("failed to open {}", zip_path.display()))?;
    let mut archive =
        ZipArchive::new(file).with_context(|| format!("failed to read {}", zip_path.display()))?;
    for idx in 0..archive.len() {
        let mut entry = archive.by_index(idx)?;
        let Some(enclosed) = entry.enclosed_name().map(|p| p.to_owned()) else {
            continue;
        };
        let out_path = destination.join(enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&out_path)
                .with_context(|| format!("failed to create {}", out_path.display()))?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let mut out_file = File::create(&out_path)
            .with_context(|| format!("failed to write {}", out_path.display()))?;
        std::io::copy(&mut entry, &mut out_file)
            .with_context(|| format!("failed to extract {}", out_path.display()))?;
    }
    Ok(())
}

fn extract_tar_gz(archive_path: &Path, destination: &Path) -> Result<()> {
    let file = File::open(archive_path)
        .with_context(|| format!("failed to open {}", archive_path.display()))?;
    let mut reader = GzDecoder::new(file);
    let mut data = Vec::new();
    reader.read_to_end(&mut data)?;
    let cursor = std::io::Cursor::new(data);
    let mut archive = Archive::new(cursor);
    archive
        .unpack(destination)
        .with_context(|| format!("failed to unpack into {}", destination.display()))?;
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
