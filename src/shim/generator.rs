use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};

use crate::config::{MNTPACK_HOME_ENV, RuntimeContext};

pub fn create_shim(
    runtime: &RuntimeContext,
    package_name: &str,
    shim_name: &str,
    binary_path: Option<&Path>,
) -> Result<()> {
    let relative_binary = binary_path.map(|path| {
        path.strip_prefix(&runtime.paths.root)
            .unwrap_or(path)
            .to_path_buf()
    });

    if cfg!(windows) {
        let shim_path = runtime.paths.bin.join(format!("{shim_name}.cmd"));
        let direct_command = relative_binary
            .as_ref()
            .map(|path| {
                format!(
                    "\"%{MNTPACK_HOME_ENV}%\\{}\" %*\r\nexit /b %errorlevel%\r\n",
                    path.to_string_lossy().replace('/', "\\")
                )
            })
            .unwrap_or_else(|| {
                format!(
                    "if exist \"%MNTPACK_CMD%\" (\r\n  \"%MNTPACK_CMD%\" run \"{package_name}\" %*\r\n  exit /b %errorlevel%\r\n)\r\necho mntpack: package has no direct binary fallback.>&2\r\nexit /b 1\r\n"
                )
            });
        let default_root = runtime.paths.root.to_string_lossy();
        let content = format!(
            "@echo off\r\nset \"{MNTPACK_HOME_ENV}=%{MNTPACK_HOME_ENV}%\"\r\nif \"%{MNTPACK_HOME_ENV}%\"==\"\" set \"{MNTPACK_HOME_ENV}={default_root}\"\r\nset \"MNTPACK_CMD=%{MNTPACK_HOME_ENV}%\\bin\\mntpack.exe\"\r\nset \"MNTPACK_CONFIG=%{MNTPACK_HOME_ENV}%\\config.json\"\r\nset \"MNTPACK_AUTO_UPDATE=0\"\r\nif exist \"%MNTPACK_CONFIG%\" (\r\n  findstr /R /I /C:\"\\\"autoUpdateOnRun\\\"[ ]*:[ ]*true\" \"%MNTPACK_CONFIG%\" >nul\r\n  if not errorlevel 1 set \"MNTPACK_AUTO_UPDATE=1\"\r\n)\r\nif \"%MNTPACK_AUTO_UPDATE%\"==\"1\" (\r\n  if exist \"%MNTPACK_CMD%\" (\r\n    \"%MNTPACK_CMD%\" run \"{package_name}\" %*\r\n    exit /b %errorlevel%\r\n  )\r\n)\r\n{direct_command}"
        );
        fs::write(&shim_path, content)
            .with_context(|| format!("failed to write shim {}", shim_path.display()))?;
        return Ok(());
    }

    let shim_path = runtime.paths.bin.join(shim_name);
    let direct_command = relative_binary
        .as_ref()
        .map(|path| {
            format!(
                "exec \"${{{MNTPACK_HOME_ENV}}}/{}\" \"$@\"\n",
                path.to_string_lossy().replace('\\', "/")
            )
        })
        .unwrap_or_else(|| {
            format!(
                "if [ -x \"$MNTPACK_CMD\" ]; then\n  exec \"$MNTPACK_CMD\" run \"{package_name}\" \"$@\"\nfi\necho \"mntpack: package has no direct binary fallback\" >&2\nexit 1\n"
            )
        });
    let default_root = runtime.paths.root.to_string_lossy();
    let content = format!(
        "#!/bin/sh\n{0}=\"${{{0}:-{1}}}\"\nMNTPACK_CMD=\"${{{0}}}/bin/mntpack\"\nMNTPACK_CONFIG=\"${{{0}}}/config.json\"\nif [ -f \"$MNTPACK_CONFIG\" ] && grep -Eq '\"autoUpdateOnRun\"[[:space:]]*:[[:space:]]*true' \"$MNTPACK_CONFIG\" 2>/dev/null; then\n  if [ -x \"$MNTPACK_CMD\" ]; then\n    exec \"$MNTPACK_CMD\" run \"{2}\" \"$@\"\n  fi\nfi\n{3}",
        MNTPACK_HOME_ENV, default_root, package_name, direct_command
    );
    fs::write(&shim_path, content)
        .with_context(|| format!("failed to write shim {}", shim_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&shim_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&shim_path, perms)?;
    }

    Ok(())
}

pub fn ensure_bin_on_path(runtime: &RuntimeContext) -> Result<bool> {
    let bin_dir = runtime.paths.bin.clone();
    let root_dir = runtime.paths.root.clone();
    let current = env::var_os("PATH").unwrap_or_default();
    let mut changed = false;
    if !path_contains(&current, &bin_dir) {
        let mut entries: Vec<PathBuf> = env::split_paths(&current).collect();
        entries.push(bin_dir.clone());
        let joined = env::join_paths(entries).context("failed to rebuild PATH variable")?;
        unsafe {
            env::set_var("PATH", &joined);
        }
        changed = true;
    }

    unsafe {
        env::set_var(MNTPACK_HOME_ENV, &root_dir);
    }

    if cfg!(windows) {
        if persist_windows_user_path(&bin_dir, &root_dir)? {
            changed = true;
        }
        let _ = refresh_windows_environment();
    } else {
        if persist_bashrc_path(&bin_dir, &root_dir)? {
            changed = true;
            let _ = source_bashrc();
        }
    }

    Ok(changed)
}

fn path_contains(path_value: &std::ffi::OsStr, needle: &Path) -> bool {
    env::split_paths(path_value).any(|entry| path_eq(&entry, needle))
}

fn path_eq(a: &Path, b: &Path) -> bool {
    let left = a
        .to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_string();
    let right = b
        .to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_string();
    if cfg!(windows) {
        left.eq_ignore_ascii_case(&right)
    } else {
        left == right
    }
}

fn persist_windows_user_path(bin_dir: &Path, root_dir: &Path) -> Result<bool> {
    let bin = bin_dir.to_string_lossy().replace('\'', "''");
    let root = root_dir.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$target='{bin}';\
         $root='{root}';\
         $existing=[Environment]::GetEnvironmentVariable('Path','User');\
         $parts=@();\
         if ($existing) {{ $parts=$existing -split ';' }};\
         $exists=$false;\
         $changed=$false;\
         foreach ($p in $parts) {{\
           if ($p.TrimEnd('\\') -ieq $target.TrimEnd('\\')) {{ $exists=$true; break }}\
         }};\
         if (-not $exists) {{\
           $newPath = if ([string]::IsNullOrWhiteSpace($existing)) {{ $target }} else {{ \"$existing;$target\" }};\
           [Environment]::SetEnvironmentVariable('Path', $newPath, 'User');\
           $changed=$true\
         }};\
         $existingHome=[Environment]::GetEnvironmentVariable('{MNTPACK_HOME_ENV}','User');\
         if ($existingHome -ne $root) {{\
           [Environment]::SetEnvironmentVariable('{MNTPACK_HOME_ENV}', $root, 'User');\
           $changed=$true\
         }};\
         if ($changed) {{ exit 10 }} else {{ exit 0 }}"
    );

    let status = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .status()
        .context("failed to persist user PATH on Windows")?;
    if status.code() == Some(10) {
        return Ok(true);
    }
    if status.success() {
        return Ok(false);
    }
    anyhow::bail!("failed to persist PATH using powershell")
}

fn refresh_windows_environment() -> Result<()> {
    let script = r#"
$sig='[DllImport("user32.dll",SetLastError=true,CharSet=CharSet.Auto)] public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);'
Add-Type -MemberDefinition $sig -Name NativeMethods -Namespace Win32 -ErrorAction SilentlyContinue | Out-Null
[UIntPtr]$out=[UIntPtr]::Zero
[Win32.NativeMethods]::SendMessageTimeout([IntPtr]0xffff,0x1A,[UIntPtr]::Zero,'Environment',2,5000,[ref]$out) | Out-Null
"#;
    let status = Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .status()
        .context("failed to refresh windows environment")?;
    if !status.success() {
        anyhow::bail!("failed to refresh windows environment");
    }
    Ok(())
}

fn persist_bashrc_path(bin_dir: &Path, root_dir: &Path) -> Result<bool> {
    let home = dirs::home_dir().context("unable to locate home directory")?;
    let bashrc = home.join(".bashrc");
    let bin = bin_dir.to_string_lossy().replace('"', "\\\"");
    let line = format!("export PATH=\"{bin}:$PATH\"");
    let root = root_dir.to_string_lossy().replace('"', "\\\"");
    let root_line = format!("export {MNTPACK_HOME_ENV}=\"{root}\"");
    let existing = if bashrc.exists() {
        fs::read_to_string(&bashrc)
            .with_context(|| format!("failed to read {}", bashrc.display()))?
    } else {
        String::new()
    };

    let mut new_content = existing;
    let mut changed = false;
    if !new_content.contains(&line) {
        if !new_content.ends_with('\n') && !new_content.is_empty() {
            new_content.push('\n');
        }
        new_content.push_str("# Added by mntpack\n");
        new_content.push_str(&line);
        new_content.push('\n');
        changed = true;
    }
    if !new_content.contains(&root_line) {
        if !new_content.ends_with('\n') && !new_content.is_empty() {
            new_content.push('\n');
        }
        new_content.push_str("# Added by mntpack\n");
        new_content.push_str(&root_line);
        new_content.push('\n');
        changed = true;
    }
    if changed {
        fs::write(&bashrc, new_content)
            .with_context(|| format!("failed to write {}", bashrc.display()))?;
    }
    Ok(changed)
}

fn source_bashrc() -> Result<()> {
    let status = Command::new("bash")
        .args(["-lc", "source ~/.bashrc >/dev/null 2>&1 || true"])
        .status()
        .context("failed to source ~/.bashrc")?;
    if !status.success() {
        anyhow::bail!("failed to source ~/.bashrc");
    }
    Ok(())
}
