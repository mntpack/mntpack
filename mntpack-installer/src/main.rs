use std::{
    collections::hash_map::DefaultHasher,
    env, fs,
    hash::{Hash, Hasher},
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};
use clap::Parser;
use serde::Serialize;

const APP_DIR: &str = ".mntpack";
const MNTPACK_HOME_ENV: &str = "MNTPACK_HOME";
const EMBEDDED_MNTPACK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/mntpack_payload.bin"));
const MNTPACK_OWNER: &str = "mntpack";
const MNTPACK_REPO: &str = "mntpack";

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
    packages: PathBuf,
    store: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallRecord {
    package_name: String,
    owner: String,
    repo: String,
    version: Option<String>,
    commit: Option<String>,
    binary_rel_path: Option<String>,
    binary_path: Option<String>,
    run_command: Option<String>,
    shim_name: Option<String>,
    store_entry: Option<String>,
    build_pending: bool,
    global: bool,
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

    configure_current_process_env(&install_paths.root, &install_paths.bin)?;
    let installed_via_existing = if cli.binary.is_none() {
        try_sync_with_existing_mntpack(&install_paths.root)?
    } else {
        false
    };

    let mut binary_path = if installed_via_existing {
        println!("used existing mntpack to sync the managed mntpack package");
        package_binary_path(&install_paths.root, &install_paths.packages)?
    } else {
        install_embedded_as_package(&install_paths, cli.binary.as_deref())?
    };

    let post_sync_ok = sync_with_installed_mntpack(&binary_path, &install_paths.root)?;
    if post_sync_ok {
        if let Ok(updated_binary) = package_binary_path(&install_paths.root, &install_paths.packages) {
            binary_path = updated_binary;
        }
    }

    let env_updated = persist_user_environment(&install_paths.root, &install_paths.bin)?;

    println!("mntpack installed at {}", binary_path.display());
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
    let cache_git = cache.join("git");
    let cache_exec = cache.join("exec");
    let store = root.join("store");
    let bin = root.join("bin");
    for dir in [
        root, &repos, &packages, &cache, &cache_git, &cache_exec, &store, &bin,
    ] {
        fs::create_dir_all(dir)
            .with_context(|| format!("failed to create directory {}", dir.display()))?;
    }

    Ok(InstallPaths {
        root: root.to_path_buf(),
        bin,
        packages,
        store,
    })
}

fn install_embedded_as_package(paths: &InstallPaths, explicit: Option<&Path>) -> Result<PathBuf> {
    let payload = load_payload_bytes(explicit)?;
    let payload_tag = short_payload_tag(&payload);
    let binary_name = if cfg!(windows) {
        "mntpack.exe"
    } else {
        "mntpack"
    };

    let store_dir = paths.store.join("mntpack").join(&payload_tag);
    fs::create_dir_all(&store_dir).with_context(|| format!("failed to create {}", store_dir.display()))?;
    let store_binary = store_dir.join(binary_name);
    fs::write(&store_binary, payload)
        .with_context(|| format!("failed to write {}", store_binary.display()))?;
    make_executable_if_unix(&store_binary)?;

    let package_dir = paths.packages.join("mntpack");
    fs::create_dir_all(&package_dir)
        .with_context(|| format!("failed to create {}", package_dir.display()))?;
    link_payload(&package_dir, &store_dir)?;
    write_install_record(&package_dir, &store_binary, binary_name, &payload_tag)?;
    create_mntpack_shim(&paths.root, &paths.bin, binary_name)?;

    Ok(store_binary)
}

fn load_payload_bytes(explicit: Option<&Path>) -> Result<Vec<u8>> {
    if let Some(source) = explicit {
        return fs::read(source)
            .with_context(|| format!("failed to read mntpack binary at {}", source.display()));
    }
    Ok(EMBEDDED_MNTPACK.to_vec())
}

fn short_payload_tag(bytes: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    let value = hasher.finish();
    format!("{value:016x}").chars().take(7).collect()
}

fn link_payload(package_dir: &Path, store_dir: &Path) -> Result<()> {
    let payload_link = package_dir.join("payload");
    if fs::symlink_metadata(&payload_link).is_ok() {
        remove_path(&payload_link)?;
    }

    if try_symlink_dir(store_dir, &payload_link).is_err() {
        fs::create_dir_all(&payload_link)
            .with_context(|| format!("failed to create {}", payload_link.display()))?;
        copy_dir_recursive(store_dir, &payload_link)?;
    }
    Ok(())
}

fn write_install_record(
    package_dir: &Path,
    store_binary: &Path,
    binary_name: &str,
    payload_tag: &str,
) -> Result<()> {
    let record = InstallRecord {
        package_name: "mntpack".to_string(),
        owner: MNTPACK_OWNER.to_string(),
        repo: MNTPACK_REPO.to_string(),
        version: None,
        commit: Some(payload_tag.to_string()),
        binary_rel_path: Some(format!("payload/{binary_name}")),
        binary_path: Some(store_binary.to_string_lossy().to_string()),
        run_command: None,
        shim_name: Some("mntpack".to_string()),
        store_entry: Some(format!("mntpack/{payload_tag}")),
        build_pending: false,
        global: true,
    };
    let path = package_dir.join("install.json");
    let payload = serde_json::to_string_pretty(&record)?;
    fs::write(&path, payload).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn create_mntpack_shim(root: &Path, bin_dir: &Path, binary_name: &str) -> Result<()> {
    fs::create_dir_all(bin_dir).with_context(|| format!("failed to create {}", bin_dir.display()))?;
    if cfg!(windows) {
        let shim_path = bin_dir.join("mntpack.cmd");
        let default_root = root.to_string_lossy();
        let content = format!(
            "@echo off\r\nsetlocal EnableExtensions EnableDelayedExpansion\r\nset \"{MNTPACK_HOME_ENV}=%{MNTPACK_HOME_ENV}%\"\r\nif \"%{MNTPACK_HOME_ENV}%\"==\"\" set \"{MNTPACK_HOME_ENV}={default_root}\"\r\n\"%{MNTPACK_HOME_ENV}%\\packages\\mntpack\\payload\\{binary_name}\" %*\r\nexit /b !ERRORLEVEL!\r\n"
        );
        fs::write(&shim_path, content)
            .with_context(|| format!("failed to write {}", shim_path.display()))?;
        return Ok(());
    }

    let shim_path = bin_dir.join("mntpack");
    let default_root = root.to_string_lossy();
    let content = format!(
        "#!/bin/sh\n{MNTPACK_HOME_ENV}=\"${{{MNTPACK_HOME_ENV}:-{default_root}}}\"\nexec \"${{{MNTPACK_HOME_ENV}}}/packages/mntpack/payload/{binary_name}\" \"$@\"\n"
    );
    fs::write(&shim_path, content)
        .with_context(|| format!("failed to write {}", shim_path.display()))?;
    make_executable_if_unix(&shim_path)?;
    Ok(())
}

fn package_binary_path(root: &Path, packages_dir: &Path) -> Result<PathBuf> {
    let package_dir = packages_dir.join("mntpack");
    let install_json = package_dir.join("install.json");
    if !install_json.exists() {
        anyhow::bail!(
            "existing mntpack sync did not produce install metadata at {}",
            install_json.display()
        );
    }
    let content = fs::read_to_string(&install_json)
        .with_context(|| format!("failed to read {}", install_json.display()))?;
    let json: serde_json::Value =
        serde_json::from_str(&content).with_context(|| format!("failed to parse {}", install_json.display()))?;
    if let Some(binary_path) = json
        .get("binaryPath")
        .and_then(|value| value.as_str())
    {
        let path = PathBuf::from(binary_path);
        if path.is_absolute() {
            return Ok(path);
        }
        return Ok(root.join(path));
    }
    let rel = json
        .get("binaryRelPath")
        .and_then(|value| value.as_str())
        .unwrap_or("payload/mntpack");
    Ok(package_dir.join(rel))
}

fn try_sync_with_existing_mntpack(root: &Path) -> Result<bool> {
    let status = Command::new("mntpack")
        .args(["sync", "mntpack/mntpack", "--name", "mntpack", "-g"])
        .env(MNTPACK_HOME_ENV, root)
        .status();
    match status {
        Ok(status) if status.success() => Ok(true),
        Ok(_) => Ok(false),
        Err(_) => Ok(false),
    }
}

fn sync_with_installed_mntpack(binary_path: &Path, root: &Path) -> Result<bool> {
    let status = Command::new(binary_path)
        .args(["sync", "mntpack/mntpack", "--name", "mntpack", "-g"])
        .env(MNTPACK_HOME_ENV, root)
        .status()
        .with_context(|| {
            format!(
                "failed to run managed mntpack self-sync using {}",
                binary_path.display()
            )
        })?;
    if status.success() {
        return Ok(true);
    }
    eprintln!(
        "warning: post-install mntpack sync failed (exit code {:?})",
        status.code()
    );
    Ok(false)
}

fn remove_path(path: &Path) -> Result<()> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    } else if metadata.is_dir() {
        fs::remove_dir_all(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<()> {
    for entry in fs::read_dir(source).with_context(|| format!("failed to read {}", source.display()))? {
        let entry = entry?;
        let source_path = entry.path();
        let dest_path = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            fs::create_dir_all(&dest_path)
                .with_context(|| format!("failed to create {}", dest_path.display()))?;
            copy_dir_recursive(&source_path, &dest_path)?;
        } else {
            fs::copy(&source_path, &dest_path).with_context(|| {
                format!(
                    "failed to copy {} -> {}",
                    source_path.display(),
                    dest_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn try_symlink_dir(target: &Path, link: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link)
            .with_context(|| format!("failed to symlink {} -> {}", link.display(), target.display()))
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
            .with_context(|| format!("failed to symlink {} -> {}", link.display(), target.display()))
    }
}

fn make_executable_if_unix(_path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(_path, perms)?;
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
