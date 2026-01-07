use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub harness: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub iterations: Option<String>,
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub dangerous: Option<bool>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

impl Config {
    pub fn load() -> Result<Option<Self>> {
        let config_path = Self::find_config_file()?;
        match config_path {
            Some(path) => {
                let content = std::fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read config file: {}", path.display()))?;
                let config: Config = toml::from_str(&content)
                    .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
                Ok(Some(config))
            }
            None => Ok(None),
        }
    }

    fn find_config_file() -> Result<Option<PathBuf>> {
        let current_dir = std::env::current_dir()?;
        let local_config = current_dir.join(".ralphrc");
        if local_config.exists() {
            return Ok(Some(local_config));
        }
        let local_toml = current_dir.join(".ralphrc.toml");
        if local_toml.exists() {
            return Ok(Some(local_toml));
        }
        if let Some(home) = dirs::home_dir() {
            let home_config = home.join(".ralphrc");
            if home_config.exists() {
                return Ok(Some(home_config));
            }
            let home_toml = home.join(".ralphrc.toml");
            if home_toml.exists() {
                return Ok(Some(home_toml));
            }
        }
        Ok(None)
    }
}
