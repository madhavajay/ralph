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
    #[serde(default)]
    pub provider: Option<String>,

    // Usage limits
    #[serde(default)]
    pub usage_limit_daily: Option<u8>,
    #[serde(default)]
    pub usage_limit_weekly: Option<u8>,
    #[serde(default)]
    pub usage_check_interval: Option<u32>,
    #[serde(default)]
    pub fallback_harness: Option<String>,

    // Tmux settings
    #[serde(default)]
    pub tmux: Option<bool>,
    #[serde(default)]
    pub tmux_session_prefix: Option<String>,
    #[serde(default)]
    pub tmux_attach: Option<bool>,

    // Monitor settings
    #[serde(default)]
    pub monitor_interval: Option<String>,
    #[serde(default)]
    pub monitor_harness: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;

    impl Config {
        fn from_toml(content: &str) -> anyhow::Result<Self> {
            toml::from_str(content).with_context(|| "Failed to parse TOML config")
        }
    }

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert!(config.harness.is_none());
        assert!(config.model.is_none());
        assert!(config.iterations.is_none());
        assert!(config.task.is_none());
        assert!(config.dangerous.is_none());
        assert!(config.reasoning_effort.is_none());
    }

    #[test]
    fn test_config_from_toml_full() {
        let toml = r#"
harness = "claude"
model = "claude-sonnet-4-20250514"
iterations = "5"
task = "TASK.md"
dangerous = true
reasoning_effort = "high"
"#;
        let config = Config::from_toml(toml).unwrap();
        assert_eq!(config.harness, Some("claude".to_string()));
        assert_eq!(config.model, Some("claude-sonnet-4-20250514".to_string()));
        assert_eq!(config.iterations, Some("5".to_string()));
        assert_eq!(config.task, Some("TASK.md".to_string()));
        assert_eq!(config.dangerous, Some(true));
        assert_eq!(config.reasoning_effort, Some("high".to_string()));
    }

    #[test]
    fn test_config_from_toml_partial() {
        let toml = r#"
harness = "codex"
dangerous = false
"#;
        let config = Config::from_toml(toml).unwrap();
        assert_eq!(config.harness, Some("codex".to_string()));
        assert!(config.model.is_none());
        assert!(config.iterations.is_none());
        assert!(config.task.is_none());
        assert_eq!(config.dangerous, Some(false));
        assert!(config.reasoning_effort.is_none());
    }

    #[test]
    fn test_config_from_toml_empty() {
        let toml = "";
        let config = Config::from_toml(toml).unwrap();
        assert!(config.harness.is_none());
        assert!(config.model.is_none());
    }

    #[test]
    fn test_config_from_toml_infinite_iterations() {
        let toml = r#"
iterations = "inf"
"#;
        let config = Config::from_toml(toml).unwrap();
        assert_eq!(config.iterations, Some("inf".to_string()));
    }

    #[test]
    fn test_config_from_toml_invalid() {
        let toml = "this is not valid toml [[[";
        let result = Config::from_toml(toml);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_usage_limits() {
        let toml = r#"
usage_limit_daily = 80
usage_limit_weekly = 90
usage_check_interval = 5
fallback_harness = "gemini"
"#;
        let config = Config::from_toml(toml).unwrap();
        assert_eq!(config.usage_limit_daily, Some(80));
        assert_eq!(config.usage_limit_weekly, Some(90));
        assert_eq!(config.usage_check_interval, Some(5));
        assert_eq!(config.fallback_harness, Some("gemini".to_string()));
    }

    #[test]
    fn test_config_tmux_settings() {
        let toml = r#"
tmux = true
tmux_session_prefix = "myralph"
tmux_attach = false
"#;
        let config = Config::from_toml(toml).unwrap();
        assert_eq!(config.tmux, Some(true));
        assert_eq!(config.tmux_session_prefix, Some("myralph".to_string()));
        assert_eq!(config.tmux_attach, Some(false));
    }

    #[test]
    fn test_config_monitor_settings() {
        let toml = r#"
monitor_interval = "5m"
monitor_harness = "claude"
"#;
        let config = Config::from_toml(toml).unwrap();
        assert_eq!(config.monitor_interval, Some("5m".to_string()));
        assert_eq!(config.monitor_harness, Some("claude".to_string()));
    }
}
