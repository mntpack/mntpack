use anyhow::{Result, bail};

use super::driver::{
    DriverRuntime, InstallContext, InstallDriver, InstallResult, manifest_bin, run_shell_command,
};

pub struct GenericDriver;

impl InstallDriver for GenericDriver {
    fn detect(&self, _repo_path: &std::path::Path) -> bool {
        true
    }

    fn install(&self, ctx: &InstallContext, _runtime: &DriverRuntime<'_>) -> Result<InstallResult> {
        let Some(manifest) = &ctx.manifest else {
            bail!("generic installs require mntpack.json");
        };

        if let Some(build_command) = &manifest.build {
            run_shell_command(build_command, &ctx.repo_path)?;
        }

        let binary = if manifest.bin.is_some() {
            Some(manifest_bin(ctx)?)
        } else if manifest.resolve_run_command().is_some() {
            None
        } else {
            bail!("generic installs require either 'run' command or 'bin' path in mntpack.json");
        };

        Ok(InstallResult {
            binary_path: binary,
            shim_name: ctx.package_name.clone(),
        })
    }
}
