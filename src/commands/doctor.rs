use std::process::Command;

use anyhow::Result;

use crate::config::RuntimeContext;

pub fn execute(runtime: &RuntimeContext) -> Result<()> {
    println!("root\t{}", runtime.paths.root.display());
    println!("config\t{}", runtime.paths.config.display());
    println!("bin\t{}", runtime.paths.bin.display());
    println!("store\t{}", runtime.paths.store.display());
    println!("cache\t{}", runtime.paths.cache.display());

    let checks = [
        ("git", runtime.config.paths.git.as_str()),
        ("python", runtime.config.paths.python.as_str()),
        ("pip", runtime.config.paths.pip.as_str()),
        ("cargo", runtime.config.paths.cargo.as_str()),
        ("node", runtime.config.paths.node.as_str()),
        ("npm", runtime.config.paths.npm.as_str()),
        ("cmake", runtime.config.paths.cmake.as_str()),
        ("make", runtime.config.paths.make.as_str()),
    ];

    for (label, tool) in checks {
        let ok = Command::new(tool).arg("--version").output().is_ok();
        let status = if ok { "ok" } else { "missing" };
        println!("{label}\t{status}\t({tool})");
    }

    Ok(())
}
