use std::{collections::HashSet, fs};

use anyhow::{Context, Result};

use crate::{config::RuntimeContext, package::record::load_all_records};

pub fn execute(runtime: &RuntimeContext, clean_unused_repos: bool) -> Result<()> {
    if runtime.paths.cache.exists() {
        fs::remove_dir_all(&runtime.paths.cache)
            .with_context(|| format!("failed to remove {}", runtime.paths.cache.display()))?;
    }
    fs::create_dir_all(&runtime.paths.cache)
        .with_context(|| format!("failed to create {}", runtime.paths.cache.display()))?;
    fs::create_dir_all(&runtime.paths.cache_git)
        .with_context(|| format!("failed to create {}", runtime.paths.cache_git.display()))?;
    fs::create_dir_all(&runtime.paths.cache_exec)
        .with_context(|| format!("failed to create {}", runtime.paths.cache_exec.display()))?;

    println!("cleared cache at {}", runtime.paths.cache.display());

    if clean_unused_repos {
        clean_repos(runtime)?;
    }

    Ok(())
}

fn clean_repos(runtime: &RuntimeContext) -> Result<()> {
    let records = load_all_records(&runtime.paths.packages)?;
    let used: HashSet<String> = records
        .iter()
        .map(|record| crate::config::repo_key(&record.owner, &record.repo))
        .collect();

    if !runtime.paths.repos.exists() {
        return Ok(());
    }

    let mut removed = 0usize;
    for entry in fs::read_dir(&runtime.paths.repos)
        .with_context(|| format!("failed to read {}", runtime.paths.repos.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let key = entry.file_name().to_string_lossy().to_string();
        if used.contains(&key) {
            continue;
        }
        fs::remove_dir_all(entry.path())
            .with_context(|| format!("failed to remove {}", entry.path().display()))?;
        removed += 1;
    }

    println!("removed {removed} unused repo clone(s)");
    Ok(())
}
