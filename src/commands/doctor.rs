use std::{
    collections::{BTreeSet, HashSet},
    process::Command,
};

use anyhow::Result;

use crate::{
    config::RuntimeContext,
    package::record::{find_record_by_package_name, load_all_records},
    shim::generator::{create_shim, ensure_bin_on_path},
};

pub async fn execute(runtime: &RuntimeContext, fix: bool) -> Result<()> {
    println!("root\t{}", runtime.paths.root.display());
    println!("config\t{}", runtime.paths.config.display());
    println!("bin\t{}", runtime.paths.bin.display());
    println!("store\t{}", runtime.paths.store.display());
    println!("cache\t{}", runtime.paths.cache.display());
    println!("nuget\t{}", runtime.paths.nuget.display());
    println!("nuget-feed\t{}", runtime.paths.nuget_feed.display());
    println!("nuget-state\t{}", runtime.paths.nuget_state.display());

    let checks = [
        ("git", runtime.config.paths.git.as_str()),
        ("python", runtime.config.paths.python.as_str()),
        ("pip", runtime.config.paths.pip.as_str()),
        ("cargo", runtime.config.paths.cargo.as_str()),
        ("node", runtime.config.paths.node.as_str()),
        ("npm", runtime.config.paths.npm.as_str()),
        ("dotnet", runtime.config.paths.dotnet.as_str()),
        ("cmake", runtime.config.paths.cmake.as_str()),
        ("make", runtime.config.paths.make.as_str()),
    ];

    let mut missing_tools = Vec::new();
    for (label, tool) in checks {
        let ok = Command::new(tool).arg("--version").output().is_ok();
        let status = if ok { "ok" } else { "missing" };
        println!("{label}\t{status}\t({tool})");
        if !ok {
            missing_tools.push(label);
        }
    }

    let records = load_all_records(&runtime.paths.packages)?;
    let mut repo_missing = Vec::new();
    let mut binary_missing = Vec::new();
    let mut shim_missing = Vec::new();

    for record in &records {
        let repo_dir = runtime
            .paths
            .repo_dir_existing_or_new(&record.owner, &record.repo);
        if !repo_dir.exists() {
            println!("repo\tmissing\t({})", record.package_name);
            repo_missing.push(record.package_name.clone());
        }

        if record.run_command.is_none() {
            let binary = crate::commands::sync::resolve_binary_path(runtime, record);
            let binary_ok = binary.as_ref().map(|path| path.exists()).unwrap_or(false);
            if !binary_ok {
                println!("binary\tmissing\t({})", record.package_name);
                binary_missing.push(record.package_name.clone());
            }
        }

        if record.global {
            let shim_name = record.shim_name.as_deref().unwrap_or(&record.package_name);
            let shim_path = if cfg!(windows) {
                runtime.paths.bin.join(format!("{shim_name}.cmd"))
            } else {
                runtime.paths.bin.join(shim_name)
            };
            if !shim_path.exists() {
                println!("shim\tmissing\t({})", record.package_name);
                shim_missing.push(record.package_name.clone());
            }
        }
    }

    if !fix {
        let issue_count = repo_missing.len() + binary_missing.len() + shim_missing.len();
        println!("issues\t{issue_count}");
        if !missing_tools.is_empty() {
            println!("missing tools:\t{}", missing_tools.join(", "));
        }
        return Ok(());
    }

    let mut repaired = 0usize;
    if ensure_bin_on_path(runtime)? {
        repaired += 1;
    }

    let mut repair_targets = BTreeSet::new();
    for package in repo_missing.iter().chain(binary_missing.iter()) {
        repair_targets.insert(package.clone());
    }

    let mut visited = HashSet::new();
    for package in &repair_targets {
        if let Some(record) = find_record_by_package_name(&runtime.paths.packages, package)? {
            crate::commands::sync::sync_package_internal(
                runtime,
                &record.repo_spec(),
                record.version.as_deref(),
                None,
                Some(&record.package_name),
                record.global,
                &mut visited,
            )
            .await?;
            repaired += 1;
        }
    }

    for package in shim_missing {
        if repair_targets.contains(&package) {
            continue;
        }
        if let Some(record) = find_record_by_package_name(&runtime.paths.packages, &package)? {
            let shim_name = record
                .shim_name
                .clone()
                .unwrap_or_else(|| record.package_name.clone());
            let binary = crate::commands::sync::resolve_binary_path(runtime, &record);
            create_shim(runtime, &record.package_name, &shim_name, binary.as_deref())?;
            repaired += 1;
        }
    }

    println!("fixed\t{repaired}");
    Ok(())
}
