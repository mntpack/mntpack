use std::collections::HashSet;

use anyhow::Result;

use crate::{
    config::RuntimeContext,
    package::lockfile::{regenerate_from_installed, save_to_cwd},
    package::record::{find_record_by_package_name, load_all_records},
};

pub async fn execute(runtime: &RuntimeContext, package: Option<&str>) -> Result<()> {
    if let Some(package_name) = package {
        if let Some(record) = find_record_by_package_name(&runtime.paths.packages, package_name)? {
            let mut visited = HashSet::new();
            crate::commands::sync::sync_package_internal(
                runtime,
                &record.repo_spec(),
                record.version.as_deref(),
                None,
                Some(&record.package_name),
                record.global,
                &mut visited,
                false,
            )
            .await?;
            println!("updated {}", record.package_name);
            let lock = regenerate_from_installed(runtime)?;
            save_to_cwd(&lock)?;
            return Ok(());
        }

        crate::commands::sync::execute(runtime, package_name, None, None, None, false).await?;
        let lock = regenerate_from_installed(runtime)?;
        save_to_cwd(&lock)?;
        return Ok(());
    }

    let records = load_all_records(&runtime.paths.packages)?;
    if records.is_empty() {
        println!("no installed packages to update");
        return Ok(());
    }

    let mut visited = HashSet::new();
    for record in &records {
        crate::commands::sync::sync_package_internal(
            runtime,
            &record.repo_spec(),
            record.version.as_deref(),
            None,
            Some(&record.package_name),
            record.global,
            &mut visited,
            false,
        )
        .await?;
    }

    let lock = regenerate_from_installed(runtime)?;
    save_to_cwd(&lock)?;
    println!("updated {} package(s)", records.len());
    Ok(())
}
