use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};
use clap::Parser;

const APP_DIR: &str = ".mntpack";
const MNTPACK_HOME_ENV: &str = "MNTPACK_HOME";
const EMBEDDED_MNTPACK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/mntpack_payload.bin"));

#[derive(Debug, Parser)]
#[command(name = "mntpack-installer", version, about = "Installs mntpack")]
struct Cli {
    #[arg(long, value_name = "PATH")]
    binary: Option<PathBuf>,
    #[arg(long, value_name = "DIR")]
    install_dir: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    non_interactive: bool,
}

#[derive(Debug, Clone)]
struct InstallPaths {
    root: PathBuf,
    bin: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let default_home = dirs::home_dir().context("unable to find home directory")?;

    let install_base = match (cli.install_dir, cli.non_interactive) {
        (Some(path), _) => path,
        (None, true) => default_home.clone(),
        (None, false) => prompt_for_install_dir(&default_home)?,
    };

    let root = install_base.join(APP_DIR);
    let install_paths = ensure_install_layout(&root)?;
    let target_binary = if cfg!(windows) {
        install_paths.bin.join("mntpack.exe")
    } else {
        install_paths.bin.join("mntpack")
    };

    install_binary(cli.binary.as_deref(), &target_binary)?;
    configure_current_process_env(&install_paths.root, &install_paths.bin)?;
    let env_updated = persist_user_environment(&install_paths.root, &install_paths.bin)?;

    println!("mntpack installed at {}", target_binary.display());
    if env_updated {
        println!(
            "PATH and {} were updated for future terminals.",
            MNTPACK_HOME_ENV
        );
    }

    Ok(())
}

fn prompt_for_install_dir(default_home: &Path) -> Result<PathBuf> {
    print!(
        "Install base directory [{}]: ",
        default_home.to_string_lossy()
    );
    io::stdout().flush().context("failed to flush prompt")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to read installer input")?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(default_home.to_path_buf());
    }
    Ok(PathBuf::from(trimmed))
}

fn ensure_install_layout(root: &Path) -> Result<InstallPaths> {
    let repos = root.join("repos");
    let packages = root.join("packages");
    let cache = root.join("cache");
    let bin = root.join("bin");
    for dir in [root, &repos, &packages, &cache, &bin] {
        fs::create_dir_all(dir)
            .with_context(|| format!("failed to create directory {}", dir.display()))?;
    }

    Ok(InstallPaths {
        root: root.to_path_buf(),
        bin,
    })
}

fn install_binary(explicit: Option<&Path>, destination: &Path) -> Result<()> {
    if let Some(source) = explicit {
        return copy_binary(source, destination);
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(destination, EMBEDDED_MNTPACK).with_context(|| {
        format!(
            "failed to write embedded mntpack to {}",
            destination.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(destination)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(destination, perms)?;
    }

    Ok(())
}

fn copy_binary(source: &Path, destination: &Path) -> Result<()> {
    if !source.exists() {
        anyhow::bail!("mntpack binary not found at {}", source.display());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(source, destination).with_context(|| {
        format!(
            "failed to copy mntpack binary {} -> {}",
            source.display(),
            destination.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(destination)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(destination, perms)?;
    }

    Ok(())
}

fn configure_current_process_env(root: &Path, bin: &Path) -> Result<()> {
    let current = env::var_os("PATH").unwrap_or_default();
    let mut entries: Vec<PathBuf> = env::split_paths(&current).collect();
    if !entries.iter().any(|entry| path_eq(entry, bin)) {
        entries.push(bin.to_path_buf());
        let joined = env::join_paths(entries).context("failed to update PATH")?;
        unsafe {
            env::set_var("PATH", joined);
        }
    }
    unsafe {
        env::set_var(MNTPACK_HOME_ENV, root);
    }
    Ok(())
}

fn persist_user_environment(root: &Path, bin: &Path) -> Result<bool> {
    if cfg!(windows) {
        persist_windows_environment(root, bin)
    } else {
        persist_unix_environment(root, bin)
    }
}

fn persist_windows_environment(root: &Path, bin: &Path) -> Result<bool> {
    let bin_s = bin.to_string_lossy().replace('\'', "''");
    let root_s = root.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$bin='{bin_s}';\
         $root='{root_s}';\
         $changed=$false;\
         $path=[Environment]::GetEnvironmentVariable('Path','User');\
         $parts=@(); if ($path) {{ $parts=$path -split ';' }};\
         $exists=$false;\
         foreach ($p in $parts) {{ if ($p.TrimEnd('\\') -ieq $bin.TrimEnd('\\')) {{ $exists=$true; break }} }};\
         if (-not $exists) {{\
           $newPath = if ([string]::IsNullOrWhiteSpace($path)) {{ $bin }} else {{ \"$path;$bin\" }};\
           [Environment]::SetEnvironmentVariable('Path', $newPath, 'User');\
           $changed=$true\
         }};\
         $existingHome=[Environment]::GetEnvironmentVariable('{MNTPACK_HOME_ENV}','User');\
         if ($existingHome -ne $root) {{\
           [Environment]::SetEnvironmentVariable('{MNTPACK_HOME_ENV}', $root, 'User');\
           $changed=$true\
         }};\
         if ($changed) {{\
           $sig='[DllImport(\"user32.dll\",SetLastError=true,CharSet=CharSet.Auto)] public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);';\
           Add-Type -MemberDefinition $sig -Name NativeMethods -Namespace Win32 -ErrorAction SilentlyContinue | Out-Null;\
           [UIntPtr]$out=[UIntPtr]::Zero;\
           [Win32.NativeMethods]::SendMessageTimeout([IntPtr]0xffff,0x1A,[UIntPtr]::Zero,'Environment',2,5000,[ref]$out) | Out-Null\
         }};\
         if ($changed) {{ exit 10 }} else {{ exit 0 }}"
    );

    let status = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .status()
        .context("failed to run powershell while persisting environment")?;
    if status.code() == Some(10) {
        return Ok(true);
    }
    if status.success() {
        return Ok(false);
    }
    anyhow::bail!("failed to persist user environment on Windows");
}

fn persist_unix_environment(root: &Path, bin: &Path) -> Result<bool> {
    let home = dirs::home_dir().context("unable to find home directory")?;
    let bashrc = home.join(".bashrc");
    let mut content = if bashrc.exists() {
        fs::read_to_string(&bashrc)
            .with_context(|| format!("failed to read {}", bashrc.display()))?
    } else {
        String::new()
    };

    let path_line = format!("export PATH=\"{}:$PATH\"", bin.display());
    let home_line = format!("export {}=\"{}\"", MNTPACK_HOME_ENV, root.display());
    let mut changed = false;

    if !content.contains(&path_line) {
        if !content.ends_with('\n') && !content.is_empty() {
            content.push('\n');
        }
        content.push_str("# Added by mntpack-installer\n");
        content.push_str(&path_line);
        content.push('\n');
        changed = true;
    }

    if !content.contains(&home_line) {
        if !content.ends_with('\n') && !content.is_empty() {
            content.push('\n');
        }
        content.push_str("# Added by mntpack-installer\n");
        content.push_str(&home_line);
        content.push('\n');
        changed = true;
    }

    if changed {
        fs::write(&bashrc, content)
            .with_context(|| format!("failed to write {}", bashrc.display()))?;
        let _ = Command::new("bash")
            .args(["-lc", "source ~/.bashrc >/dev/null 2>&1 || true"])
            .status();
    }

    Ok(changed)
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
