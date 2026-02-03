use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::notify::{NotifyConfig, PauseConfig, PauseStrategy};
use crate::task::TaskConfig;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    // Core settings (flat, for backwards compatibility)
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

    // Usage limits (flat, for backwards compatibility)
    #[serde(default)]
    pub usage_limit_daily: Option<u8>,
    #[serde(default)]
    pub usage_limit_weekly: Option<u8>,
    #[serde(default)]
    pub usage_check_interval: Option<u32>,
    #[serde(default)]
    pub fallback_harness: Option<String>,

    // Tmux settings (flat)
    #[serde(default)]
    pub tmux: Option<bool>,
    #[serde(default)]
    pub tmux_session_prefix: Option<String>,
    #[serde(default)]
    pub tmux_attach: Option<bool>,

    // Monitor settings (flat)
    #[serde(default)]
    pub monitor_interval: Option<String>,
    #[serde(default)]
    pub monitor_harness: Option<String>,

    // New section-based configuration
    /// [loop] section for loop control
    #[serde(default, rename = "loop")]
    pub loop_config: LoopConfigSection,

    /// [task] section for task parsing
    #[serde(default)]
    pub task_config: TaskConfig,

    /// [pause] section for pause strategies
    #[serde(default)]
    pub pause: PauseConfig,

    /// [notify] section for notifications
    #[serde(default)]
    pub notify: NotifyConfig,

    /// [harnesses] section for multi-harness configuration
    #[serde(default)]
    pub harnesses: HarnessesConfig,

    /// [limits] section for per-harness limits
    #[serde(default)]
    pub limits: LimitsConfig,

    /// [circuit_breaker] section
    #[serde(default)]
    pub circuit_breaker: CircuitBreakerConfig,

    /// [hooks] section for validation and other hooks
    #[serde(default)]
    pub hooks: HooksConfig,
}

/// Loop configuration section
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoopConfigSection {
    /// Enable loop mode
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Maximum iterations (None for infinite)
    #[serde(default)]
    pub max_iterations: Option<u32>,
    /// Task file path
    #[serde(default)]
    pub task_file: Option<String>,
    /// Validation command after each iteration
    #[serde(default)]
    pub validate_cmd: Option<String>,
    /// Checkpoint interval in seconds
    #[serde(default)]
    pub checkpoint_interval: Option<u64>,
}

/// Multi-harness configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HarnessesConfig {
    /// Primary harness to use
    #[serde(default)]
    pub primary: Option<String>,
    /// Fallback harness when primary is limited
    #[serde(default)]
    pub fallback: Option<String>,
    /// Model for primary harness
    #[serde(default)]
    pub primary_model: Option<String>,
    /// Model for fallback harness
    #[serde(default)]
    pub fallback_model: Option<String>,
}

/// Per-harness usage limits
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LimitsConfig {
    /// Codex daily limit percentage
    #[serde(default)]
    pub codex_daily: Option<u8>,
    /// Codex weekly limit percentage
    #[serde(default)]
    pub codex_weekly: Option<u8>,
    /// Claude daily limit percentage
    #[serde(default)]
    pub claude_daily: Option<u8>,
    /// Claude weekly limit percentage
    #[serde(default)]
    pub claude_weekly: Option<u8>,
    /// Gemini daily limit percentage
    #[serde(default)]
    pub gemini_daily: Option<u8>,
    /// Gemini weekly limit percentage
    #[serde(default)]
    pub gemini_weekly: Option<u8>,
    /// Pi daily limit percentage
    #[serde(default)]
    pub pi_daily: Option<u8>,
    /// Pi weekly limit percentage
    #[serde(default)]
    pub pi_weekly: Option<u8>,
}

impl LimitsConfig {
    /// Get daily limit for a specific harness
    pub fn get_daily(&self, harness: &str) -> Option<u8> {
        match harness.to_lowercase().as_str() {
            "codex" => self.codex_daily,
            "claude" => self.claude_daily,
            "gemini" => self.gemini_daily,
            "pi" => self.pi_daily,
            _ => None,
        }
    }

    /// Get weekly limit for a specific harness
    pub fn get_weekly(&self, harness: &str) -> Option<u8> {
        match harness.to_lowercase().as_str() {
            "codex" => self.codex_weekly,
            "claude" => self.claude_weekly,
            "gemini" => self.gemini_weekly,
            "pi" => self.pi_weekly,
            _ => None,
        }
    }
}

/// Circuit breaker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Number of iterations without progress before tripping
    #[serde(default = "default_no_progress_threshold")]
    pub no_progress_threshold: u32,
    /// Number of same errors before tripping
    #[serde(default = "default_same_error_threshold")]
    pub same_error_threshold: u32,
    /// Cooldown time in seconds after tripping
    #[serde(default = "default_cooldown_seconds")]
    pub cooldown_seconds: u64,
    /// Per-task-type overrides
    #[serde(default)]
    pub overrides: HashMap<String, CircuitBreakerOverride>,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            no_progress_threshold: default_no_progress_threshold(),
            same_error_threshold: default_same_error_threshold(),
            cooldown_seconds: default_cooldown_seconds(),
            overrides: HashMap::new(),
        }
    }
}

fn default_no_progress_threshold() -> u32 {
    5
}

fn default_same_error_threshold() -> u32 {
    3
}

fn default_cooldown_seconds() -> u64 {
    300
}

/// Override circuit breaker settings for specific task types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerOverride {
    #[serde(default)]
    pub no_progress_threshold: Option<u32>,
    #[serde(default)]
    pub same_error_threshold: Option<u32>,
    #[serde(default)]
    pub cooldown_seconds: Option<u64>,
}

/// Hooks configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HooksConfig {
    /// Command to run after each iteration
    #[serde(default)]
    pub after_each: Option<String>,
    /// Strategy when after_each hook fails
    #[serde(default)]
    pub after_each_on_fail: Option<HookFailStrategy>,
    /// Timeout for after_each hook in seconds
    #[serde(default)]
    pub after_each_timeout: Option<u64>,
    /// Command to run on error
    #[serde(default)]
    pub on_error: Option<String>,
    /// Timeout for on_error hook in seconds
    #[serde(default)]
    pub on_error_timeout: Option<u64>,
    /// Command to run on completion
    #[serde(default)]
    pub on_complete: Option<String>,
    /// Timeout for on_complete hook in seconds
    #[serde(default)]
    pub on_complete_timeout: Option<u64>,
}

/// Strategy for handling hook failures
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum HookFailStrategy {
    /// Continue execution, log warning
    #[default]
    Continue,
    /// Stop the loop
    Stop,
    /// Retry the current task
    Retry { max: u32 },
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

    /// Get the effective harness name (from harnesses.primary or harness)
    pub fn effective_harness(&self) -> Option<&str> {
        self.harnesses
            .primary
            .as_deref()
            .or(self.harness.as_deref())
    }

    /// Get the effective fallback harness
    pub fn effective_fallback(&self) -> Option<&str> {
        self.harnesses
            .fallback
            .as_deref()
            .or(self.fallback_harness.as_deref())
    }

    /// Get effective daily limit for a harness
    pub fn effective_daily_limit(&self, harness: &str) -> Option<u8> {
        self.limits.get_daily(harness).or(self.usage_limit_daily)
    }

    /// Get effective weekly limit for a harness
    pub fn effective_weekly_limit(&self, harness: &str) -> Option<u8> {
        self.limits.get_weekly(harness).or(self.usage_limit_weekly)
    }

    /// Get the effective pause strategy
    pub fn effective_pause_strategy(&self) -> PauseStrategy {
        self.pause.strategy.clone()
    }

    /// Check if loop mode is enabled
    pub fn loop_enabled(&self) -> bool {
        self.loop_config.enabled.unwrap_or(false)
    }

    /// Get effective max iterations
    pub fn effective_max_iterations(&self) -> Option<u32> {
        self.loop_config.max_iterations.or_else(|| {
            self.iterations
                .as_ref()
                .and_then(|s| if s == "inf" { None } else { s.parse().ok() })
        })
    }

    /// Get effective task file
    #[allow(dead_code)]
    pub fn effective_task_file(&self) -> Option<&str> {
        self.loop_config
            .task_file
            .as_deref()
            .or(self.task.as_deref())
    }

    /// Get effective checkpoint interval
    pub fn effective_checkpoint_interval(&self) -> u64 {
        self.loop_config.checkpoint_interval.unwrap_or(60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_config_loop_section() {
        let toml = r#"
[loop]
enabled = true
max_iterations = 200
task_file = "fix_plan.md"
validate_cmd = "./validate.sh"
checkpoint_interval = 120
"#;
        let config = Config::from_toml(toml).unwrap();
        assert_eq!(config.loop_config.enabled, Some(true));
        assert_eq!(config.loop_config.max_iterations, Some(200));
        assert_eq!(
            config.loop_config.task_file,
            Some("fix_plan.md".to_string())
        );
        assert_eq!(
            config.loop_config.validate_cmd,
            Some("./validate.sh".to_string())
        );
        assert_eq!(config.loop_config.checkpoint_interval, Some(120));
    }

    #[test]
    fn test_config_harnesses_section() {
        let toml = r#"
[harnesses]
primary = "codex"
fallback = "claude"
primary_model = "gpt-5.2-codex"
fallback_model = "claude-opus-4-5"
"#;
        let config = Config::from_toml(toml).unwrap();
        assert_eq!(config.harnesses.primary, Some("codex".to_string()));
        assert_eq!(config.harnesses.fallback, Some("claude".to_string()));
        assert_eq!(
            config.harnesses.primary_model,
            Some("gpt-5.2-codex".to_string())
        );
        assert_eq!(
            config.harnesses.fallback_model,
            Some("claude-opus-4-5".to_string())
        );
    }

    #[test]
    fn test_config_limits_section() {
        let toml = r#"
[limits]
codex_daily = 80
codex_weekly = 90
claude_daily = 90
claude_weekly = 95
"#;
        let config = Config::from_toml(toml).unwrap();
        assert_eq!(config.limits.codex_daily, Some(80));
        assert_eq!(config.limits.codex_weekly, Some(90));
        assert_eq!(config.limits.claude_daily, Some(90));
        assert_eq!(config.limits.claude_weekly, Some(95));

        // Test helper methods
        assert_eq!(config.limits.get_daily("codex"), Some(80));
        assert_eq!(config.limits.get_weekly("claude"), Some(95));
        assert_eq!(config.limits.get_daily("unknown"), None);
    }

    #[test]
    fn test_config_circuit_breaker_section() {
        let toml = r#"
[circuit_breaker]
no_progress_threshold = 10
same_error_threshold = 5
cooldown_seconds = 600

[circuit_breaker.overrides.search]
no_progress_threshold = 15

[circuit_breaker.overrides.build]
same_error_threshold = 2
"#;
        let config = Config::from_toml(toml).unwrap();
        assert_eq!(config.circuit_breaker.no_progress_threshold, 10);
        assert_eq!(config.circuit_breaker.same_error_threshold, 5);
        assert_eq!(config.circuit_breaker.cooldown_seconds, 600);

        let search = config.circuit_breaker.overrides.get("search").unwrap();
        assert_eq!(search.no_progress_threshold, Some(15));

        let build = config.circuit_breaker.overrides.get("build").unwrap();
        assert_eq!(build.same_error_threshold, Some(2));
    }

    #[test]
    fn test_config_hooks_section() {
        let toml = r#"
[hooks]
after_each = "./validate.sh"
after_each_on_fail = "continue"
after_each_timeout = 60
on_error = "./notify.sh"
on_complete = "./summary.sh"
"#;
        let config = Config::from_toml(toml).unwrap();
        assert_eq!(config.hooks.after_each, Some("./validate.sh".to_string()));
        assert_eq!(
            config.hooks.after_each_on_fail,
            Some(HookFailStrategy::Continue)
        );
        assert_eq!(config.hooks.after_each_timeout, Some(60));
        assert_eq!(config.hooks.on_error, Some("./notify.sh".to_string()));
        assert_eq!(config.hooks.on_complete, Some("./summary.sh".to_string()));
    }

    #[test]
    fn test_config_notify_section() {
        let toml = r#"
[notify]
webhook = "https://hooks.slack.com/..."
paused_file = true
timeout_secs = 60
"#;
        let config = Config::from_toml(toml).unwrap();
        assert_eq!(
            config.notify.webhook,
            Some("https://hooks.slack.com/...".to_string())
        );
        assert!(config.notify.paused_file);
        assert_eq!(config.notify.timeout_secs, 60);
    }

    #[test]
    fn test_config_effective_methods() {
        let toml = r#"
harness = "codex"
fallback_harness = "claude"
usage_limit_daily = 80

[harnesses]
primary = "gemini"
fallback = "pi"

[limits]
gemini_daily = 70
"#;
        let config = Config::from_toml(toml).unwrap();

        // harnesses.primary takes precedence over harness
        assert_eq!(config.effective_harness(), Some("gemini"));
        assert_eq!(config.effective_fallback(), Some("pi"));

        // limits.gemini_daily takes precedence for gemini
        assert_eq!(config.effective_daily_limit("gemini"), Some(70));
        // Falls back to usage_limit_daily for others
        assert_eq!(config.effective_daily_limit("codex"), Some(80));
    }

    #[test]
    fn test_config_full_example() {
        let toml = r#"
# Core settings
harness = "codex"
model = "gpt-5.2-codex"
task = "@fix_plan.md"
dangerous = true

# Loop settings
[loop]
enabled = true
max_iterations = 200
task_file = "fix_plan.md"
checkpoint_interval = 60

# Harness configuration
[harnesses]
primary = "codex"
fallback = "claude"

# Per-harness limits
[limits]
codex_daily = 80
claude_daily = 90

# Pause strategy
[pause]
strategy = { type = "fallback", harness = "claude" }

# Notifications
[notify]
webhook = "https://example.com/webhook"
paused_file = true

# Circuit breaker
[circuit_breaker]
no_progress_threshold = 5
same_error_threshold = 3

# Hooks
[hooks]
after_each = "./validate.sh"
after_each_on_fail = "continue"
"#;
        let config = Config::from_toml(toml).unwrap();
        assert!(config.loop_config.enabled.unwrap());
        assert_eq!(config.effective_harness(), Some("codex"));
        assert_eq!(config.effective_fallback(), Some("claude"));
    }
}
