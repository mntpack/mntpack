use std::collections::HashSet;

use anyhow::{Result, bail};

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
                None,
                Some("auto"),
                Some(&record.package_name),
                record.global,
                &mut visited,
                false,
            )
            .await?;
            println!("upgraded {}", record.package_name);
            let lock = regenerate_from_installed(runtime)?;
            save_to_cwd(&lock)?;
            return Ok(());
        }

        bail!("package '{package_name}' is not installed");
    }

    let records = load_all_records(&runtime.paths.packages)?;
    if records.is_empty() {
        println!("no installed packages to upgrade");
        return Ok(());
    }

    let mut visited = HashSet::new();
    let mut upgraded = 0usize;
    let mut failed = 0usize;
    for record in &records {
        let result = crate::commands::sync::sync_package_internal(
            runtime,
            &record.repo_spec(),
            None,
            Some("auto"),
            Some(&record.package_name),
            record.global,
            &mut visited,
            false,
        )
        .await;
        match result {
            Ok(_) => upgraded += 1,
            Err(err) => {
                failed += 1;
                eprintln!("warning: failed to upgrade {}: {err}", record.package_name);
            }
        }
    }

    if failed > 0 {
        let lock = regenerate_from_installed(runtime)?;
        save_to_cwd(&lock)?;
        eprintln!("upgraded {upgraded} package(s), {failed} failed");
        return Ok(());
    }

    let lock = regenerate_from_installed(runtime)?;
    save_to_cwd(&lock)?;
    println!("upgraded {upgraded} package(s)");
    Ok(())
}
