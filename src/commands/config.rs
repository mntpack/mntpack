use anyhow::{Result, bail};

use crate::{
    cli::ConfigAction,
    config::{Config, RuntimeContext},
};

pub fn execute(runtime: &RuntimeContext, action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Show => {
            println!("{}", serde_json::to_string_pretty(&runtime.config)?);
        }
        ConfigAction::Get { key } => {
            let value = get_value(&runtime.config, &key)?;
            println!("{value}");
        }
        ConfigAction::Set { key, value } => {
            let mut config = runtime.config.clone();
            set_value(&mut config, &key, &value)?;
            runtime.save_config(&config)?;
            println!("updated {key} = {value}");
        }
        ConfigAction::Reset => {
            let config = Config::default();
            runtime.save_config(&config)?;
            println!("config reset to defaults");
        }
    }
    Ok(())
}

fn get_value(config: &Config, key: &str) -> Result<String> {
    match normalize_key(key).as_str() {
        "defaultowner" => Ok(config.default_owner.clone()),
        "pathsgit" => Ok(config.paths.git.clone()),
        "pathspython" => Ok(config.paths.python.clone()),
        "pathspip" => Ok(config.paths.pip.clone()),
        "pathsnode" => Ok(config.paths.node.clone()),
        "pathsnpm" => Ok(config.paths.npm.clone()),
        "pathscargo" => Ok(config.paths.cargo.clone()),
        "pathsdotnet" => Ok(config.paths.dotnet.clone()),
        "pathscmake" => Ok(config.paths.cmake.clone()),
        "pathsmake" => Ok(config.paths.make.clone()),
        "autoupdateonrun" => Ok(config.auto_update_on_run.to_string()),
        "binarycacheenabled" => Ok(config.binary_cache.enabled.to_string()),
        "binarycacherepo" => Ok(config.binary_cache.repo.clone().unwrap_or_default()),
        "syncdispatchenabled" => Ok(config.sync_dispatch.enabled.to_string()),
        "syncdispatchrepo" => Ok(config.sync_dispatch.repo.clone()),
        "syncdispatchtokenenv" => Ok(config.sync_dispatch.token_env.clone()),
        "syncdispatcheventtype" => Ok(config.sync_dispatch.event_type.clone()),
        _ => bail!("unknown config key '{key}'"),
    }
}

fn set_value(config: &mut Config, key: &str, value: &str) -> Result<()> {
    match normalize_key(key).as_str() {
        "defaultowner" => config.default_owner = value.to_string(),
        "pathsgit" => config.paths.git = value.to_string(),
        "pathspython" => config.paths.python = value.to_string(),
        "pathspip" => config.paths.pip = value.to_string(),
        "pathsnode" => config.paths.node = value.to_string(),
        "pathsnpm" => config.paths.npm = value.to_string(),
        "pathscargo" => config.paths.cargo = value.to_string(),
        "pathsdotnet" => config.paths.dotnet = value.to_string(),
        "pathscmake" => config.paths.cmake = value.to_string(),
        "pathsmake" => config.paths.make = value.to_string(),
        "autoupdateonrun" => {
            config.auto_update_on_run = value
                .parse::<bool>()
                .map_err(|_| anyhow::anyhow!("expected true/false for '{}'", key))?
        }
        "binarycacheenabled" => {
            config.binary_cache.enabled = value
                .parse::<bool>()
                .map_err(|_| anyhow::anyhow!("expected true/false for '{}'", key))?
        }
        "binarycacherepo" => {
            let trimmed = value.trim();
            config.binary_cache.repo = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        "syncdispatchenabled" => {
            config.sync_dispatch.enabled = value
                .parse::<bool>()
                .map_err(|_| anyhow::anyhow!("expected true/false for '{}'", key))?
        }
        "syncdispatchrepo" => config.sync_dispatch.repo = value.trim().to_string(),
        "syncdispatchtokenenv" => config.sync_dispatch.token_env = value.trim().to_string(),
        "syncdispatcheventtype" => config.sync_dispatch.event_type = value.trim().to_string(),
        _ => bail!("unknown config key '{key}'"),
    }
    Ok(())
}

fn normalize_key(key: &str) -> String {
    key.trim().to_ascii_lowercase().replace(['.', '_', '-'], "")
}
