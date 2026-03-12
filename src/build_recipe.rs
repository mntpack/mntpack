use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::ui::progress::ProgressBar;

pub const DEFAULT_RECIPE_FILE: &str = "mntpack.yml";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildRecipe {
    pub version: u32,
    pub name: Option<String>,
    pub description: Option<String>,
    pub env: BTreeMap<String, String>,
    pub steps: Vec<BuildStep>,
}

impl Default for BuildRecipe {
    fn default() -> Self {
        Self {
            version: 1,
            name: None,
            description: None,
            env: BTreeMap::new(),
            steps: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BuildStep {
    pub name: Option<String>,
    pub run: String,
    pub cwd: Option<String>,
    pub env: BTreeMap<String, String>,
}

pub fn default_recipe_path() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    Ok(cwd.join(DEFAULT_RECIPE_FILE))
}

pub fn resolve_recipe_path(input: Option<&str>) -> Result<PathBuf> {
    let Some(input) = input.map(str::trim).filter(|value| !value.is_empty()) else {
        return default_recipe_path();
    };

    let path = PathBuf::from(input);
    if path.is_absolute() {
        return Ok(path);
    }

    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    Ok(cwd.join(path))
}

pub fn generate_recipe_template(path: &Path) -> Result<()> {
    if path.exists() {
        bail!("recipe file already exists at {}", path.display());
    }

    let base_dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(base_dir)
        .with_context(|| format!("failed to create {}", base_dir.display()))?;

    let recipe = suggested_recipe(base_dir)?;
    let yaml = serde_yaml::to_string(&recipe).context("failed to serialize recipe template")?;
    fs::write(path, yaml).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn run_recipe_file(path: &Path) -> Result<()> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let recipe = serde_yaml::from_str::<BuildRecipe>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    execute_recipe(
        &recipe,
        path.parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
    )
}

fn execute_recipe(recipe: &BuildRecipe, base_dir: PathBuf) -> Result<()> {
    if recipe.steps.is_empty() {
        bail!("recipe has no build steps");
    }

    let label = recipe.name.as_deref().unwrap_or("build");
    let mut progress = ProgressBar::new(label, recipe.steps.len());

    for (index, step) in recipe.steps.iter().enumerate() {
        let step_name = step
            .name
            .clone()
            .unwrap_or_else(|| format!("step {}", index + 1));
        run_step(step, &recipe.env, &base_dir)
            .with_context(|| format!("build recipe step {} ('{}') failed", index + 1, step_name))?;
        progress.advance(step_name);
    }

    progress.finish("completed");
    Ok(())
}

fn run_step(
    step: &BuildStep,
    recipe_env: &BTreeMap<String, String>,
    base_dir: &Path,
) -> Result<()> {
    let command = step.run.trim();
    if command.is_empty() {
        bail!("build step contains an empty 'run' command");
    }

    let step_dir = resolve_step_dir(step.cwd.as_deref(), base_dir)?;
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", command]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", command]);
        c
    };

    cmd.current_dir(&step_dir);
    for (key, value) in recipe_env {
        cmd.env(key, value);
    }
    for (key, value) in &step.env {
        cmd.env(key, value);
    }

    let status = cmd
        .status()
        .with_context(|| format!("failed to run '{}' in {}", command, step_dir.display()))?;
    if !status.success() {
        bail!(
            "command '{}' failed with exit code {:?}",
            command,
            status.code()
        );
    }

    Ok(())
}

fn resolve_step_dir(step_cwd: Option<&str>, base_dir: &Path) -> Result<PathBuf> {
    let Some(step_cwd) = step_cwd.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(base_dir.to_path_buf());
    };

    let cwd_path = PathBuf::from(step_cwd);
    let resolved = if cwd_path.is_absolute() {
        cwd_path
    } else {
        base_dir.join(cwd_path)
    };
    if !resolved.exists() {
        bail!("step cwd does not exist: {}", resolved.display());
    }
    Ok(resolved)
}

fn suggested_recipe(base_dir: &Path) -> Result<BuildRecipe> {
    let project_name = base_dir
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToString::to_string)
        .unwrap_or_else(|| "project".to_string());
    let mut recipe = BuildRecipe {
        name: Some(project_name),
        description: Some("Generated by mntpack build --generate".to_string()),
        ..BuildRecipe::default()
    };

    if base_dir.join("Cargo.toml").exists() {
        recipe.steps.push(BuildStep {
            name: Some("cargo build release".to_string()),
            run: "cargo build --release".to_string(),
            ..BuildStep::default()
        });
        return Ok(recipe);
    }

    if base_dir.join("package.json").exists() {
        recipe.steps.push(BuildStep {
            name: Some("npm install".to_string()),
            run: "npm install".to_string(),
            ..BuildStep::default()
        });
        recipe.steps.push(BuildStep {
            name: Some("npm build".to_string()),
            run: "npm run build".to_string(),
            ..BuildStep::default()
        });
        return Ok(recipe);
    }

    if base_dir.join("pyproject.toml").exists() || base_dir.join("requirements.txt").exists() {
        recipe.steps.push(BuildStep {
            name: Some("python build".to_string()),
            run: "python -m build".to_string(),
            ..BuildStep::default()
        });
        return Ok(recipe);
    }

    recipe.steps.push(BuildStep {
        name: Some("edit me".to_string()),
        run: "echo add your build steps here".to_string(),
        ..BuildStep::default()
    });
    Ok(recipe)
}
