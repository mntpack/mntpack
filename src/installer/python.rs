use anyhow::Result;

use super::driver::{
    DriverRuntime, InstallContext, InstallDriver, InstallResult, manifest_bin, run_command,
};

pub struct PythonDriver;

impl InstallDriver for PythonDriver {
    fn detect(&self, repo_path: &std::path::Path) -> bool {
        repo_path.join("requirements.txt").exists() || repo_path.join("pyproject.toml").exists()
    }

    fn install(&self, ctx: &InstallContext, runtime: &DriverRuntime<'_>) -> Result<InstallResult> {
        let requirements = ctx.repo_path.join("requirements.txt");
        if requirements.exists() {
            run_command(
                &runtime.runtime.config.paths.pip,
                &["install", "-r", "requirements.txt"],
                &ctx.repo_path,
            )?;
        }

        let binary = if ctx.manifest.as_ref().and_then(|m| m.bin.as_ref()).is_some() {
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
