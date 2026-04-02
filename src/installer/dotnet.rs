use anyhow::Result;

use crate::dotnet;

use super::driver::{
    DriverRuntime, InstallContext, InstallDriver, InstallResult, manifest_bin,
    manifest_uses_command_launch, run_shell_command,
};

pub struct DotnetDriver;

impl InstallDriver for DotnetDriver {
    fn detect(&self, repo_path: &std::path::Path) -> bool {
        dotnet::is_dotnet_project(repo_path)
    }

    fn install(&self, ctx: &InstallContext, runtime: &DriverRuntime<'_>) -> Result<InstallResult> {
        dotnet::ensure_workspace_config(runtime.runtime, &ctx.repo_path, None)?;

        if let Some(manifest) = &ctx.manifest {
            if !manifest.nuget.is_empty() {
                dotnet::apply_manifest_packages(runtime.runtime, &ctx.repo_path, None, manifest)?;
            }
        }

        if let Some(build_command) = ctx
            .manifest
            .as_ref()
            .and_then(|manifest| manifest.build.as_deref())
        {
            run_shell_command(build_command, &ctx.repo_path)?;
        } else {
            dotnet::build(runtime.runtime, &ctx.repo_path)?;
        }

        if ctx
            .manifest
            .as_ref()
            .and_then(|m| m.resolve_bin_path())
            .is_some()
        {
            let bin = manifest_bin(ctx)?;
            return Ok(InstallResult {
                shim_name: infer_shim_name(&bin, &ctx.package_name),
                binary_path: Some(bin),
            });
        }

        if manifest_uses_command_launch(ctx) {
            return Ok(InstallResult {
                shim_name: ctx.package_name.clone(),
                binary_path: None,
            });
        }

        Ok(InstallResult {
            binary_path: None,
            shim_name: ctx.package_name.clone(),
        })
    }
}

fn infer_shim_name(binary: &std::path::Path, fallback: &str) -> String {
    binary
        .file_stem()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}
