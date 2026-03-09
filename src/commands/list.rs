use anyhow::Result;

use crate::{config::RuntimeContext, package::record::load_all_records};

pub fn execute(runtime: &RuntimeContext, global_only: bool) -> Result<()> {
    let records = load_all_records(&runtime.paths.packages)?;
    if records.is_empty() {
        println!("no packages installed");
        return Ok(());
    }

    if global_only {
        let mut printed = 0usize;
        for record in records.into_iter().filter(|record| record.global) {
            let shim = record
                .shim_name
                .clone()
                .unwrap_or_else(|| record.package_name.clone());
            println!("{shim} -> {}", record.package_name);
            printed += 1;
        }
        if printed == 0 {
            println!("no global packages installed");
        }
        return Ok(());
    }

    for record in records {
        let version = record.version.as_deref().unwrap_or("latest");
        let scope = if record.global { "global" } else { "local" };
        println!(
            "{}\t{}\t{}\t{}",
            record.package_name,
            version,
            scope,
            record.repo_spec()
        );
    }

    Ok(())
}
