use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::{
    config::RuntimeContext,
    package::record::{PackageRecord, find_record_by_package_name},
};

pub fn execute(runtime: &RuntimeContext, package: &str) -> Result<()> {
    let Some(record) = find_record_by_package_name(&runtime.paths.packages, package)? else {
        bail!("package '{package}' is not installed");
    };

    let install_path = runtime.paths.package_dir(&record.package_name);
    let repo_path = runtime
        .paths
        .repo_dir(&crate::config::repo_key(&record.owner, &record.repo));
    let shim_path = resolve_shim_path(runtime, &record);

    println!("Package: {}", record.package_name);
    println!("Repository: {}", record.repo_spec());
    println!(
        "Installed Version: {}",
        record.version.as_deref().unwrap_or("latest")
    );
    println!("Commit: {}", record.commit.as_deref().unwrap_or("unknown"));
    println!("Install Path: {}", install_path.display());
    println!("Repo Path: {}", repo_path.display());
    if record.global {
        match shim_path {
            Some(path) => println!("Shim: {}", path.display()),
            None => println!("Shim: (not found)"),
        }
    } else {
        println!("Shim: (local install)");
    }
    println!(
        "Run Command: {}",
        record.run_command.as_deref().unwrap_or("(none)")
    );
    println!("Auto Update On Run: {}", runtime.config.auto_update_on_run);
    println!("Build Pending: {}", record.build_pending);
    if let Some(store_entry) = record.store_entry.as_deref() {
        println!(
            "Store Entry: {}",
            runtime.paths.store.join(store_entry).display()
        );
    }

    Ok(())
}

fn resolve_shim_path(runtime: &RuntimeContext, record: &PackageRecord) -> Option<PathBuf> {
    let mut candidates = vec![];
    if let Some(shim_name) = record.shim_name.as_deref() {
        candidates.push(shim_name.to_string());
    }
    candidates.push(record.package_name.clone());

    for candidate in candidates {
        let path = if cfg!(windows) {
            runtime.paths.bin.join(format!("{candidate}.cmd"))
        } else {
            runtime.paths.bin.join(candidate)
        };
        if path.exists() {
            return Some(path);
        }
    }
    None
}
