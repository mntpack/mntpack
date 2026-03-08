use std::fs;

use anyhow::{Result, bail};

use crate::{config::RuntimeContext, package::record::load_all_records};

pub fn execute(runtime: &RuntimeContext, command: &str) -> Result<()> {
    let records = load_all_records(&runtime.paths.packages)?;
    if records.is_empty() {
        bail!("no packages installed");
    }

    let command_name = command.trim();
    if command_name.is_empty() {
        bail!("command cannot be empty");
    }

    let shim_path = if cfg!(windows) {
        runtime.paths.bin.join(format!("{command_name}.cmd"))
    } else {
        runtime.paths.bin.join(command_name)
    };

    if !shim_path.exists() {
        bail!(
            "command '{}' is not provided by mntpack shims",
            command_name
        );
    }

    let provided_package = shim_package_name(&shim_path).or_else(|| {
        records
            .iter()
            .find(|record| {
                record.shim_name.as_deref() == Some(command_name) || record.package_name == command
            })
            .map(|record| record.package_name.clone())
    });

    let Some(package_name) = provided_package else {
        bail!("unable to resolve package for command '{}'", command_name);
    };
    let Some(record) = records
        .iter()
        .find(|record| record.package_name == package_name)
    else {
        bail!("unable to resolve metadata for package '{}'", package_name);
    };

    println!("{command_name} -> {}", record.repo_spec());
    println!("Shim: {}", shim_path.display());
    println!(
        "Repo: {}",
        runtime
            .paths
            .repo_dir(&crate::config::repo_key(&record.owner, &record.repo))
            .display()
    );
    Ok(())
}

fn shim_package_name(shim_path: &std::path::Path) -> Option<String> {
    let content = fs::read_to_string(shim_path).ok()?;
    let marker = "run \"";
    let start = content.find(marker)?;
    let remaining = &content[start + marker.len()..];
    let end = remaining.find('"')?;
    let package = remaining[..end].trim();
    if package.is_empty() {
        return None;
    }
    Some(package.to_string())
}
