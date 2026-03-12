use anyhow::Result;

use crate::build_recipe::{generate_recipe_template, resolve_recipe_path, run_recipe_file};

pub fn execute(recipe: Option<&str>, generate: bool) -> Result<()> {
    let path = resolve_recipe_path(recipe)?;
    if generate {
        generate_recipe_template(&path)?;
        println!("generated build recipe at {}", path.display());
        return Ok(());
    }

    run_recipe_file(&path)?;
    println!("build recipe completed: {}", path.display());
    Ok(())
}
