use anyhow::{Result, bail};

use super::driver::{
    DriverRuntime, InstallContext, InstallDriver, InstallResult, auto_discover_binary,
    manifest_bin, run_shell_command,
};

pub struct GenericDriver;

impl InstallDriver for GenericDriver {
    fn detect(&self, _repo_path: &std::path::Path) -> bool {
        true
    }

    fn install(&self, ctx: &InstallContext, _runtime: &DriverRuntime<'_>) -> Result<InstallResult> {
        if let Some(manifest) = &ctx.manifest {
            if let Some(build_command) = &manifest.build {
                run_shell_command(build_command, &ctx.repo_path)?;
            }

            let binary = if manifest.resolve_bin_path().is_some() {
                Some(manifest_bin(ctx)?)
            } else if manifest.resolve_run_command().is_some()
                || manifest.resolve_bin_command().is_some()
            {
                None
            } else {
                auto_discover_binary(&ctx.repo_path, &ctx.package_name)?
            };

            return Ok(InstallResult {
                binary_path: binary,
                shim_name: ctx.package_name.clone(),
            });
        }

        let binary = auto_discover_binary(&ctx.repo_path, &ctx.package_name)?;
        if binary.is_none() {
            bail!(
                "generic install could not determine executable; add mntpack.json or produce a binary in target/release, bin, dist, or build"
            );
        }

        Ok(InstallResult {
            binary_path: binary,
            shim_name: ctx.package_name.clone(),
        })
    }
}
