use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use walkdir::WalkDir;

use crate::{config::RuntimeContext, package::manifest::Manifest};

#[derive(Debug, Clone)]
pub struct InstallContext {
    pub package_name: String,
    pub repo_path: PathBuf,
    pub package_dir: PathBuf,
    pub manifest: Option<Manifest>,
}

pub struct DriverRuntime<'a> {
    pub runtime: &'a RuntimeContext,
}

#[derive(Debug, Clone)]
pub struct InstallResult {
    pub binary_path: Option<PathBuf>,
    pub shim_name: String,
}

pub trait InstallDriver: Send + Sync {
    fn detect(&self, repo_path: &Path) -> bool;
    fn install(&self, ctx: &InstallContext, runtime: &DriverRuntime<'_>) -> Result<InstallResult>;
}

pub fn run_command(program: &str, args: &[&str], cwd: &Path) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .status()
        .with_context(|| format!("failed to start '{program}' in {}", cwd.display()))?;

    if !status.success() {
        bail!(
            "command '{}' failed with exit code {:?}",
            format_command(program, args),
            status.code()
        );
    }

    Ok(())
}

pub fn run_shell_command(command: &str, cwd: &Path) -> Result<()> {
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", command]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", command]);
        c
    };
    let status = cmd
        .current_dir(cwd)
        .status()
        .with_context(|| format!("failed to run script '{command}' in {}", cwd.display()))?;
    if !status.success() {
        bail!(
            "script '{command}' failed with exit code {:?}",
            status.code()
        );
    }
    Ok(())
}

pub fn run_command_with_args(
    command: &str,
    args: &[String],
    command_root: &Path,
    invocation_cwd: &Path,
) -> Result<()> {
    if let Some((program, base_args)) = parse_simple_command(command) {
        let base_args = absolutize_existing_relative_args(&base_args, command_root);
        let status = Command::new(&program)
            .args(&base_args)
            .args(args)
            .current_dir(invocation_cwd)
            .status()
            .with_context(|| {
                format!(
                    "failed to run '{}' in {}",
                    command,
                    invocation_cwd.display()
                )
            })?;
        if !status.success() {
            bail!(
                "script '{}' failed with exit code {:?}",
                command,
                status.code()
            );
        }
        return Ok(());
    }

    if args.is_empty() {
        return run_shell_command(command, command_root);
    }

    let command = append_args(command, args);
    run_shell_command(&command, command_root)
}

pub fn manifest_bin(ctx: &InstallContext) -> Result<PathBuf> {
    let Some(manifest) = &ctx.manifest else {
        bail!("mntpack.json is required to determine install binary");
    };
    let Some(bin) = manifest.resolve_bin_path() else {
        bail!("mntpack.json missing required 'bin' field");
    };
    Ok(ctx.repo_path.join(bin))
}

pub fn manifest_uses_command_launch(ctx: &InstallContext) -> bool {
    ctx.manifest
        .as_ref()
        .map(|manifest| {
            manifest.resolve_run_command().is_some() || manifest.resolve_bin_command().is_some()
        })
        .unwrap_or(false)
}

fn format_command(program: &str, args: &[&str]) -> String {
    if args.is_empty() {
        return program.to_string();
    }
    format!("{program} {}", args.join(" "))
}

fn parse_simple_command(command: &str) -> Option<(String, Vec<String>)> {
    if contains_shell_metacharacters(command) {
        return None;
    }

    let tokens = tokenize_command(command)?;
    let (program, args) = tokens.split_first()?;
    Some((program.clone(), args.to_vec()))
}

fn contains_shell_metacharacters(command: &str) -> bool {
    command
        .chars()
        .any(|ch| matches!(ch, '|' | '&' | ';' | '<' | '>' | '(' | ')'))
}

fn tokenize_command(command: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;

    for ch in command.chars() {
        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }

        match ch {
            '"' | '\'' => quote = Some(ch),
            ' ' | '\t' | '\r' | '\n' => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if quote.is_some() {
        return None;
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    Some(tokens)
}

fn append_args(base_command: &str, args: &[String]) -> String {
    if args.is_empty() {
        return base_command.to_string();
    }
    let escaped: Vec<String> = args.iter().map(|arg| shell_escape(arg)).collect();
    format!("{base_command} {}", escaped.join(" "))
}

fn absolutize_existing_relative_args(args: &[String], command_root: &Path) -> Vec<String> {
    args.iter()
        .map(|arg| {
            if arg.starts_with('-') {
                return arg.clone();
            }

            let candidate = Path::new(arg);
            if candidate.is_absolute() {
                return arg.clone();
            }

            let repo_relative = command_root.join(candidate);
            if repo_relative.exists() {
                return repo_relative.to_string_lossy().to_string();
            }

            arg.clone()
        })
        .collect()
}

fn shell_escape(input: &str) -> String {
    if cfg!(windows) {
        if input.chars().all(|ch| {
            ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | '\\' | ':')
        }) {
            input.to_string()
        } else {
            format!("\"{}\"", input.replace('"', "\\\""))
        }
    } else {
        format!("'{}'", input.replace('\'', "'\"'\"'"))
    }
}

pub fn auto_discover_binary(repo_path: &Path, package_name: &str) -> Result<Option<PathBuf>> {
    let search_roots = [
        repo_path.join("target").join("release"),
        repo_path.join("bin"),
        repo_path.join("dist"),
        repo_path.join("build"),
        repo_path.to_path_buf(),
    ];

    let mut candidates = Vec::new();
    for root in search_roots {
        if !root.exists() {
            continue;
        }
        for entry in WalkDir::new(root).max_depth(5).into_iter().flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path().to_path_buf();
            if !is_executable_candidate(&path)? {
                continue;
            }
            candidates.push(path);
        }
    }

    if candidates.is_empty() {
        return Ok(None);
    }

    if let Some(path) = candidates.iter().find(|path| {
        path.file_stem()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case(package_name))
            .unwrap_or(false)
    }) {
        return Ok(Some(path.clone()));
    }

    if candidates.len() == 1 {
        return Ok(Some(candidates.remove(0)));
    }

    candidates.sort();
    Ok(candidates.first().cloned())
}

fn is_executable_candidate(path: &Path) -> Result<bool> {
    if cfg!(windows) {
        return Ok(path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("exe"))
            .unwrap_or(false));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path)?.permissions().mode();
        return Ok(mode & 0o111 != 0);
    }

    #[allow(unreachable_code)]
    Ok(false)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use tempfile::tempdir;

    use super::{
        absolutize_existing_relative_args, contains_shell_metacharacters, parse_simple_command,
    };

    #[test]
    fn parses_simple_dotnet_run_command() {
        let parsed = parse_simple_command("dotnet run --project .\\src\\Tool --")
            .expect("expected command to parse");
        assert_eq!(parsed.0, "dotnet");
        assert_eq!(
            parsed.1,
            vec![
                "run".to_string(),
                "--project".to_string(),
                ".\\src\\Tool".to_string(),
                "--".to_string()
            ]
        );
    }

    #[test]
    fn rejects_shell_style_commands_for_direct_execution() {
        assert!(contains_shell_metacharacters(
            "dotnet restore && dotnet build"
        ));
        assert!(parse_simple_command("dotnet restore && dotnet build").is_none());
    }

    #[test]
    fn absolutizes_existing_repo_relative_args() {
        let temp = tempdir().expect("tempdir");
        let project_dir = temp.path().join("src").join("Tool");
        fs::create_dir_all(&project_dir).expect("project dir");
        let project_path = project_dir.join("Tool.csproj");
        fs::write(&project_path, "").expect("project file");

        let args = vec![
            "run".to_string(),
            "--project".to_string(),
            ".\\src\\Tool\\Tool.csproj".to_string(),
            "--".to_string(),
        ];
        let resolved = absolutize_existing_relative_args(&args, temp.path());

        assert_eq!(resolved[0], "run");
        assert_eq!(resolved[1], "--project");
        assert_eq!(Path::new(&resolved[2]), project_path.as_path());
        assert_eq!(resolved[3], "--");
    }
}
