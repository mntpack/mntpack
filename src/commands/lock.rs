use anyhow::Result;

use crate::{
    config::RuntimeContext,
    package::lockfile::{regenerate_from_installed, save_to_cwd},
};

pub fn regenerate(runtime: &RuntimeContext) -> Result<()> {
    let lock = regenerate_from_installed(runtime)?;
    save_to_cwd(&lock)?;
    println!(
        "regenerated mntpack.lock with {} package(s)",
        lock.packages.len()
    );
    Ok(())
}
