use anyhow::Result;

use crate::{
    config::RuntimeContext,
    package::{
        record::{find_record_by_package_name, load_all_records},
        resolver::resolve_repo,
    },
};

const SPECIAL_PACKAGE_NAME: &str = "mntpack";
const SPECIAL_OWNER: &str = "mntpack";
const SPECIAL_REPO: &str = "mntpack";

pub async fn execute(
    runtime: &RuntimeContext,
    package_or_repo: &str,
    version: Option<&str>,
    release: Option<&str>,
    name: Option<&str>,
    global: bool,
) -> Result<()> {
    if let Some(record) = find_record_by_package_name(&runtime.paths.packages, package_or_repo)? {
        reinstall_record(runtime, &record, version, release, name, global).await?;
        println!("reinstalled {}", record.package_name);
        return Ok(());
    }

    let resolved = resolve_repo(package_or_repo, &runtime.config.default_owner)?;
    let records = load_all_records(&runtime.paths.packages)?;
    let matching: Vec<_> = records
        .into_iter()
        .filter(|record| record.owner == resolved.owner && record.repo == resolved.repo)
        .collect();

    if matching.is_empty() {
        crate::commands::sync::execute(runtime, package_or_repo, version, release, name, global)
            .await?;
        return Ok(());
    }

    for record in matching {
        reinstall_record(runtime, &record, version, release, name, global).await?;
        println!("reinstalled {}", record.package_name);
    }
    Ok(())
}

async fn reinstall_record(
    runtime: &RuntimeContext,
    record: &crate::package::record::PackageRecord,
    version: Option<&str>,
    release: Option<&str>,
    name: Option<&str>,
    global: bool,
) -> Result<()> {
    let is_special = record
        .package_name
        .eq_ignore_ascii_case(SPECIAL_PACKAGE_NAME)
        && record.owner.eq_ignore_ascii_case(SPECIAL_OWNER)
        && record.repo.eq_ignore_ascii_case(SPECIAL_REPO);
    if !is_special {
        crate::commands::remove::execute(runtime, &record.package_name)?;
    }

    crate::commands::sync::execute(
        runtime,
        &record.repo_spec(),
        version.or(record.version.as_deref()),
        release,
        name.or(Some(record.package_name.as_str())),
        global || record.global,
    )
    .await
}
