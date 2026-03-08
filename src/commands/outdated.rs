use anyhow::Result;

use crate::{
    config::RuntimeContext,
    github::clone::{default_remote_commit, default_remote_commit_short, fetch_repo, head_commit},
    package::record::load_all_records,
};

pub fn execute(runtime: &RuntimeContext) -> Result<()> {
    let records = load_all_records(&runtime.paths.packages)?;
    if records.is_empty() {
        println!("no packages installed");
        return Ok(());
    }

    for record in records {
        let repo_dir = runtime
            .paths
            .repo_dir(&crate::config::repo_key(&record.owner, &record.repo));
        if !repo_dir.exists() {
            println!("{}\trepo missing", record.package_name);
            continue;
        }

        if let Err(err) = fetch_repo(&repo_dir) {
            println!("{}\tunable to fetch ({err})", record.package_name);
            continue;
        }

        let local_commit = head_commit(&repo_dir);
        let remote_commit = default_remote_commit(&repo_dir);
        let local_short = record
            .commit
            .clone()
            .or_else(|| local_commit.as_ref().ok().map(|value| short(value)));
        let remote_short = default_remote_commit_short(&repo_dir)
            .ok()
            .or_else(|| remote_commit.as_ref().ok().map(|value| short(value)));

        match (local_commit, remote_commit, local_short, remote_short) {
            (Ok(local), Ok(remote), Some(local_label), Some(remote_label)) => {
                if local == remote {
                    println!("{}\tup to date ({})", record.package_name, local_label);
                } else {
                    println!(
                        "{}\t{} -> {}",
                        record.package_name, local_label, remote_label
                    );
                }
            }
            _ => {
                println!("{}\tunable to determine status", record.package_name);
            }
        }
    }

    Ok(())
}

fn short(value: &str) -> String {
    value.chars().take(7).collect()
}
