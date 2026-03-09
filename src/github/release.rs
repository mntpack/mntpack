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
    manifest: Option<&Manifest>,
    version: Option<&str>,
    release_asset: Option<&str>,
) -> Result<Option<PathBuf>> {
    let target = current_target();
    let release_cfg = manifest.and_then(|manifest| manifest.release.get(target));
    let requested = release_asset
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let requested_auto = requested
        .map(|value| value.eq_ignore_ascii_case("auto"))
        .unwrap_or(false);
    let asset_name = if requested_auto {
        None
    } else {
        requested
            .map(|name| name.to_string())
            .or_else(|| release_cfg.map(|cfg| cfg.file.clone()))
    };

    if asset_name.is_none() && !requested_auto {
        return Ok(None);
    }

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

    let selected = if requested_auto {
        select_auto_asset(&release.assets, resolved, target)
    } else {
        let expected_asset_name = asset_name.as_deref().unwrap_or_default();
        release
            .assets
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(expected_asset_name))
    };

    let Some(asset) = selected else {
        if requested_auto {
            eprintln!(
                "warning: no auto-matched release asset for {} on {}",
                resolved.key, target
            );
        }
        return Ok(None);
    };

    let binary =
        download_and_extract_asset(runtime, resolved, asset, expected_bin.as_deref()).await?;
    Ok(Some(binary))
}

fn select_auto_asset<'a>(
    assets: &'a [ReleaseAsset],
    resolved: &ResolvedRepo,
    target: &str,
) -> Option<&'a ReleaseAsset> {
    let (os_tokens, arch_tokens) = target_tokens(target);
    let mut ranked: Vec<(&ReleaseAsset, i32)> = Vec::new();

    for asset in assets {
        let lower = asset.name.to_ascii_lowercase();
        if is_non_binary_asset(&lower) {
            continue;
        }

        let mut score = 0i32;
        if os_tokens.iter().any(|token| lower.contains(token)) {
            score += 4;
        }
        if arch_tokens.iter().any(|token| lower.contains(token)) {
            score += 3;
        }
        if lower.contains(&resolved.repo.to_ascii_lowercase()) {
            score += 1;
        }
        if is_preferred_binary_extension(&lower) {
            score += 1;
        }

        if score > 0 {
            ranked.push((asset, score));
        }
    }

    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.name.cmp(&b.0.name)));
    ranked.first().map(|(asset, _)| *asset)
}

fn target_tokens(target: &str) -> (&'static [&'static str], &'static [&'static str]) {
    match target {
        "windows-x64" => (&["windows", "win"], &["x86_64", "x64", "amd64", "win64"]),
        "windows-x86" => (&["windows", "win"], &["x86", "i386", "386", "32"]),
        "linux-x64" => (&["linux"], &["x86_64", "x64", "amd64"]),
        "linux-arm64" => (&["linux"], &["arm64", "aarch64"]),
        "macos-x64" => (
            &["macos", "mac", "darwin", "osx"],
            &["x86_64", "x64", "amd64"],
        ),
        "macos-arm64" => (&["macos", "mac", "darwin", "osx"], &["arm64", "aarch64"]),
        _ => (&[] as &[&str], &[] as &[&str]),
    }
}

fn is_non_binary_asset(name: &str) -> bool {
    let deny = [
        "sha256",
        "sha512",
        "checksum",
        "checksums",
        ".sig",
        ".asc",
        ".sum",
        "source code",
    ];
    deny.iter().any(|needle| name.contains(needle))
}

fn is_preferred_binary_extension(name: &str) -> bool {
    name.ends_with(".zip")
        || name.ends_with(".tar.gz")
        || name.ends_with(".tgz")
        || name.ends_with(".tar.xz")
        || name.ends_with(".exe")
        || name.ends_with(".msi")
        || name.ends_with(".appimage")
}

async fn download_and_extract_asset(
    runtime: &RuntimeContext,
    resolved: &ResolvedRepo,
    asset: &ReleaseAsset,
    expected_bin: Option<&str>,
) -> Result<PathBuf> {
    let cache_key = cache_safe_key(&resolved.key);
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
        .join(format!("{cache_key}-{}", asset.name));
    tokio::fs::write(&asset_path, &bytes)
        .await
        .with_context(|| format!("failed to write {}", asset_path.display()))?;

    let extract_dir = runtime.paths.cache.join(format!("extract-{cache_key}"));
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

fn cache_safe_key(value: &str) -> String {
    value
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
