use std::collections::HashSet;

use anyhow::{Result, bail};

use crate::{
    config::RuntimeContext,
    package::record::{find_record_by_package_name, load_all_records},
    ui::progress::ProgressBar,
};

pub async fn execute(runtime: &RuntimeContext, package: Option<&str>) -> Result<()> {
    if let Some(package_name) = package {
        let mut progress = ProgressBar::new("upgrade", 1);
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
            )
            .await?;
            progress.advance(format!("synced {}", record.package_name));
            println!("upgraded {}", record.package_name);
            progress.finish("done");
            return Ok(());
        }

        bail!("package '{package_name}' is not installed");
    }

    let records = load_all_records(&runtime.paths.packages)?;
    if records.is_empty() {
        println!("no installed packages to upgrade");
        return Ok(());
    }

    let mut progress = ProgressBar::new("upgrade", records.len());
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
        )
        .await;
        match result {
            Ok(_) => {
                upgraded += 1;
                progress.advance(format!("ok {}", record.package_name));
            }
            Err(err) => {
                failed += 1;
                progress.advance(format!("failed {}", record.package_name));
                eprintln!("warning: failed to upgrade {}: {err}", record.package_name);
            }
        }
    }

    if failed > 0 {
        progress.finish(format!("{upgraded} ok, {failed} failed"));
        eprintln!("upgraded {upgraded} package(s), {failed} failed");
        return Ok(());
    }

    progress.finish(format!("{upgraded} package(s)"));
    println!("upgraded {upgraded} package(s)");
    Ok(())
}
