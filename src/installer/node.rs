use anyhow::Result;

use super::driver::{
    DriverRuntime, InstallContext, InstallDriver, InstallResult, manifest_bin, run_command,
};

pub struct NodeDriver;

impl InstallDriver for NodeDriver {
    fn detect(&self, repo_path: &std::path::Path) -> bool {
        repo_path.join("package.json").exists()
    }

    fn install(&self, ctx: &InstallContext, runtime: &DriverRuntime<'_>) -> Result<InstallResult> {
        run_command(
            &runtime.runtime.config.paths.npm,
            &["install"],
            &ctx.repo_path,
        )?;

        let binary = if ctx
            .manifest
            .as_ref()
            .and_then(|m| m.resolve_bin_path())
            .is_some()
        {
            Some(manifest_bin(ctx)?)
        } else {
            None
        };

        Ok(InstallResult {
            binary_path: binary,
            shim_name: ctx.package_name.clone(),
        })
    }
}
