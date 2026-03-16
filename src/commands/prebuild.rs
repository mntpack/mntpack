use std::collections::HashSet;

use anyhow::{Context, Result, bail};

use crate::{
    binary_cache,
    config::RuntimeContext,
    package::{resolver::resolve_repo, store::sha256_file},
    ui::progress::ProgressBar,
};

pub async fn execute(runtime: &RuntimeContext) -> Result<()> {
    if !binary_cache::enabled(runtime) {
        bail!("binary cache is not enabled; set config key 'binaryCache.enabled' to true");
    }
    if !binary_cache::configured(runtime) {
        bail!(
            "binary cache repo is not configured; set config key 'binaryCache.repo' or 'syncDispatch.repo'"
        );
    }

    let mut progress = ProgressBar::new("prebuild", 5);
    let repo_spec = detect_current_repo_spec(&runtime.config.default_owner)?;
    progress.advance(format!("repo: {repo_spec}"));

    let mut visited = HashSet::new();
    let synced = crate::commands::sync::sync_package_internal(
        runtime,
        &repo_spec,
        None,
        None,
        None,
        false,
        &mut visited,
    )
    .await?;
    progress.advance(format!("synced {}", synced.package_name));

    let prepared =
        crate::commands::sync::ensure_package_ready(runtime, &synced.package_name).await?;
    progress.advance(format!("prepared {}", prepared.package_name));

    let binary_path =
        crate::commands::sync::resolve_binary_path(runtime, &prepared).with_context(|| {
            format!(
                "package '{}' has no binary to prebuild (command-only package)",
                prepared.package_name
            )
        })?;
    let hash = if let Some(hash) = prepared.binary_hash.as_deref() {
        hash.to_string()
    } else {
        sha256_file(&binary_path)?
    };
    progress.advance(format!("hash {}", short_hash(&hash)));

    binary_cache::upload_binary_to_cache(runtime, &prepared.repo_spec(), &hash, &binary_path)?;
    progress.finish("uploaded");

    println!(
        "prebuilt {} ({}) -> {}",
        prepared.package_name,
        prepared.repo_spec(),
        hash
    );
    Ok(())
}

fn detect_current_repo_spec(default_owner: &str) -> Result<String> {
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    let repo = git2::Repository::discover(&cwd)
        .with_context(|| format!("failed to locate git repository from {}", cwd.display()))?;
    let remote = repo
        .find_remote("origin")
        .context("failed to read git remote 'origin'")?;
    let url = remote
        .url()
        .context("origin remote URL is missing")?
        .to_string();
    let resolved = resolve_repo(&url, default_owner)?;
    Ok(resolved.key)
}

fn short_hash(hash: &str) -> String {
    hash.chars().take(12).collect()
}
