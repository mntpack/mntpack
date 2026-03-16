use std::collections::HashSet;

use anyhow::Result;

use crate::{
    config::RuntimeContext,
    package::record::{find_record_by_package_name, load_all_records},
    ui::progress::ProgressBar,
};

pub async fn execute(runtime: &RuntimeContext, package: Option<&str>) -> Result<()> {
    if let Some(package_name) = package {
        let mut progress = ProgressBar::new("update", 1);
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
            )
            .await?;
            progress.advance(format!("synced {}", record.package_name));
            println!("updated {}", record.package_name);
            progress.finish("done");
            return Ok(());
        }

        crate::commands::sync::execute(runtime, package_name, None, None, None, false).await?;
        progress.advance(format!("synced {package_name}"));
        progress.finish("done");
        return Ok(());
    }

    let records = load_all_records(&runtime.paths.packages)?;
    if records.is_empty() {
        println!("no installed packages to update");
        return Ok(());
    }

    let mut progress = ProgressBar::new("update", records.len());
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
        )
        .await?;
        progress.advance(record.package_name.clone());
    }

    progress.finish(format!("{} package(s)", records.len()));
    println!("updated {} package(s)", records.len());
    Ok(())
}
