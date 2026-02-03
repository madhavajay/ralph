//! Notification and pause strategy handling
//!
//! Provides notification capabilities for usage limits, errors, and other events.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{info, warn};

/// Pause strategy when usage limits are reached
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[derive(Default)]
pub enum PauseStrategy {
    /// Exit immediately
    #[default]
    Exit,
    /// Wait until a specific time
    Wait {
        /// Time to wait in seconds, or until reset time
        until_reset: bool,
    },
    /// Switch to fallback harness
    Fallback {
        /// Harness to switch to
        harness: String,
    },
    /// Send notification and then execute another strategy
    Notify {
        /// Webhook URL to send notification to
        #[serde(default)]
        webhook: Option<String>,
        /// Write a PAUSED file
        #[serde(default)]
        paused_file: bool,
        /// Strategy to execute after notification
        then: Box<PauseStrategy>,
    },
}

/// Notification configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyConfig {
    /// Webhook URL for notifications
    #[serde(default)]
    pub webhook: Option<String>,
    /// Write PAUSED file when pausing
    #[serde(default)]
    pub paused_file: bool,
    /// Custom headers for webhook requests
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
    /// Timeout for webhook requests in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    30
}

impl Default for NotifyConfig {
    fn default() -> Self {
        Self {
            webhook: None,
            paused_file: false,
            headers: std::collections::HashMap::new(),
            timeout_secs: default_timeout(),
        }
    }
}

/// Pause configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PauseConfig {
    /// Strategy when usage limit is reached
    #[serde(default)]
    pub strategy: PauseStrategy,
    /// Fallback harness when using Fallback strategy
    #[serde(default)]
    pub fallback_harness: Option<String>,
}

/// Notification event types
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(dead_code)]
pub enum NotifyEvent {
    /// Usage limit reached
    UsageLimitReached {
        harness: String,
        limit_type: String, // "daily" or "weekly"
        usage_percent: u8,
        limit_percent: u8,
    },
    /// Loop paused
    LoopPaused {
        run_id: String,
        reason: String,
        current_task: Option<String>,
        iterations_completed: u32,
    },
    /// Loop resumed
    LoopResumed {
        run_id: String,
        iterations_completed: u32,
    },
    /// Loop completed
    LoopCompleted {
        run_id: String,
        total_iterations: u32,
        tasks_completed: usize,
        total_tokens: u64,
        estimated_cost_usd: f64,
        runtime_human: String,
    },
    /// Error occurred
    Error {
        run_id: Option<String>,
        message: String,
        current_task: Option<String>,
    },
    /// Harness switched
    HarnessSwitched {
        from_harness: String,
        to_harness: String,
        reason: String,
    },
    /// Circuit breaker triggered
    CircuitBreakerTriggered {
        run_id: String,
        reason: String,
        failures_count: u32,
    },
}

/// Notification manager
pub struct Notifier {
    config: NotifyConfig,
    state_dir: PathBuf,
}

impl Notifier {
    /// Create a new notifier
    pub fn new(config: NotifyConfig, state_dir: PathBuf) -> Self {
        Self { config, state_dir }
    }

    /// Create with default state directory
    #[allow(dead_code)]
    pub fn with_default_dir(config: NotifyConfig) -> Self {
        let state_dir = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".ralph");
        Self::new(config, state_dir)
    }

    /// Send a notification event
    pub async fn notify(&self, event: &NotifyEvent) -> Result<()> {
        // Send webhook if configured
        if let Some(ref webhook) = self.config.webhook {
            if let Err(e) = self.send_webhook(webhook, event).await {
                warn!("Failed to send webhook notification: {}", e);
            }
        }

        // Write paused file if configured and event is pause-related
        if self.config.paused_file {
            if let NotifyEvent::LoopPaused { .. } | NotifyEvent::UsageLimitReached { .. } = event {
                if let Err(e) = self.write_paused_file(event) {
                    warn!("Failed to write PAUSED file: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Send a webhook notification
    async fn send_webhook(&self, url: &str, event: &NotifyEvent) -> Result<()> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(self.config.timeout_secs))
            .build()
            .context("Failed to create HTTP client")?;

        let payload = serde_json::to_value(event).context("Failed to serialize event")?;

        let mut request = client.post(url).json(&payload);

        // Add custom headers
        for (key, value) in &self.config.headers {
            request = request.header(key, value);
        }

        let response = request.send().await.context("Failed to send webhook")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Webhook returned error {}: {}", status, body);
        }

        info!("Webhook notification sent successfully");
        Ok(())
    }

    /// Write a PAUSED file
    fn write_paused_file(&self, event: &NotifyEvent) -> Result<()> {
        if !self.state_dir.exists() {
            std::fs::create_dir_all(&self.state_dir)?;
        }

        let paused_file = self.state_dir.join("PAUSED");
        let content = match event {
            NotifyEvent::LoopPaused {
                reason,
                run_id,
                current_task,
                iterations_completed,
            } => {
                format!(
                    "Paused: {}\nRun ID: {}\nCurrent Task: {}\nIterations: {}\nTime: {}\n",
                    reason,
                    run_id,
                    current_task.as_deref().unwrap_or("none"),
                    iterations_completed,
                    chrono::Utc::now().to_rfc3339()
                )
            }
            NotifyEvent::UsageLimitReached {
                harness,
                limit_type,
                usage_percent,
                limit_percent,
            } => {
                format!(
                    "Usage Limit Reached\nHarness: {}\nType: {}\nUsage: {}%\nLimit: {}%\nTime: {}\n",
                    harness,
                    limit_type,
                    usage_percent,
                    limit_percent,
                    chrono::Utc::now().to_rfc3339()
                )
            }
            _ => format!("Paused at {}\n", chrono::Utc::now().to_rfc3339()),
        };

        std::fs::write(&paused_file, content)
            .with_context(|| format!("Failed to write PAUSED file: {}", paused_file.display()))?;

        info!("PAUSED file written to {}", paused_file.display());
        Ok(())
    }

    /// Remove the PAUSED file
    pub fn clear_paused_file(&self) -> Result<()> {
        let paused_file = self.state_dir.join("PAUSED");
        if paused_file.exists() {
            std::fs::remove_file(&paused_file).with_context(|| {
                format!("Failed to remove PAUSED file: {}", paused_file.display())
            })?;
            info!("PAUSED file removed");
        }
        Ok(())
    }

    /// Check if currently paused
    #[allow(dead_code)]
    pub fn is_paused(&self) -> bool {
        self.state_dir.join("PAUSED").exists()
    }

    /// Get the PAUSED file content if it exists
    #[allow(dead_code)]
    pub fn get_paused_info(&self) -> Option<String> {
        let paused_file = self.state_dir.join("PAUSED");
        std::fs::read_to_string(paused_file).ok()
    }
}

/// Check if a PAUSED file exists in the given directory
#[allow(dead_code)]
pub fn check_paused_file(dir: &Path) -> bool {
    dir.join("PAUSED").exists()
}

/// Synchronous webhook send (for use in non-async contexts)
#[allow(dead_code)]
pub fn send_webhook_sync(url: &str, event: &NotifyEvent, timeout_secs: u64) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .context("Failed to create HTTP client")?;

    let payload = serde_json::to_value(event).context("Failed to serialize event")?;

    let response = client
        .post(url)
        .json(&payload)
        .send()
        .context("Failed to send webhook")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        anyhow::bail!("Webhook returned error {}: {}", status, body);
    }

    Ok(())
}

/// Execute a pause strategy
pub async fn execute_pause_strategy(
    strategy: &PauseStrategy,
    notifier: &Notifier,
    context: PauseContext<'_>,
) -> Result<PauseAction> {
    match strategy {
        PauseStrategy::Exit => Ok(PauseAction::Exit),
        PauseStrategy::Wait { until_reset } => {
            if *until_reset {
                // Would need reset time from usage info
                Ok(PauseAction::Wait { seconds: 3600 }) // Default 1 hour
            } else {
                Ok(PauseAction::Wait { seconds: 300 }) // Default 5 minutes
            }
        }
        PauseStrategy::Fallback { harness } => Ok(PauseAction::SwitchHarness {
            harness: harness.clone(),
        }),
        PauseStrategy::Notify {
            webhook,
            paused_file,
            then,
        } => {
            // Create and send notification event
            let event = NotifyEvent::UsageLimitReached {
                harness: context.harness.to_string(),
                limit_type: context.limit_type.to_string(),
                usage_percent: context.usage_percent,
                limit_percent: context.limit_percent,
            };

            // Override notifier config if specified
            let notify_config = NotifyConfig {
                webhook: webhook.clone().or_else(|| notifier.config.webhook.clone()),
                paused_file: *paused_file || notifier.config.paused_file,
                ..notifier.config.clone()
            };
            let temp_notifier = Notifier::new(notify_config, notifier.state_dir.clone());

            temp_notifier.notify(&event).await?;

            // Execute the "then" strategy
            Box::pin(execute_pause_strategy(then, notifier, context)).await
        }
    }
}

/// Context for pause strategy execution
#[derive(Clone)]
pub struct PauseContext<'a> {
    pub harness: &'a str,
    pub limit_type: &'a str,
    pub usage_percent: u8,
    pub limit_percent: u8,
}

/// Result of pause strategy execution
#[derive(Debug, Clone)]
pub enum PauseAction {
    /// Exit the loop
    Exit,
    /// Wait for a period
    Wait { seconds: u64 },
    /// Switch to a different harness
    SwitchHarness { harness: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pause_strategy_default() {
        let strategy = PauseStrategy::default();
        assert_eq!(strategy, PauseStrategy::Exit);
    }

    #[test]
    fn test_pause_strategy_serialize() {
        let strategy = PauseStrategy::Fallback {
            harness: "claude".to_string(),
        };
        let json = serde_json::to_string(&strategy).unwrap();
        assert!(json.contains("fallback"));
        assert!(json.contains("claude"));
    }

    #[test]
    fn test_notify_event_serialize() {
        let event = NotifyEvent::UsageLimitReached {
            harness: "codex".to_string(),
            limit_type: "daily".to_string(),
            usage_percent: 85,
            limit_percent: 80,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("usage_limit_reached"));
        assert!(json.contains("codex"));
    }

    #[test]
    fn test_notify_event_loop_completed() {
        let event = NotifyEvent::LoopCompleted {
            run_id: "test-123".to_string(),
            total_iterations: 100,
            tasks_completed: 50,
            total_tokens: 1000000,
            estimated_cost_usd: 10.50,
            runtime_human: "2h 30m".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("loop_completed"));
        assert!(json.contains("test-123"));
    }

    #[test]
    fn test_pause_strategy_nested() {
        let strategy = PauseStrategy::Notify {
            webhook: Some("https://example.com/hook".to_string()),
            paused_file: true,
            then: Box::new(PauseStrategy::Fallback {
                harness: "claude".to_string(),
            }),
        };

        let json = serde_json::to_string(&strategy).unwrap();
        assert!(json.contains("notify"));
        assert!(json.contains("example.com"));
        assert!(json.contains("fallback"));
    }

    #[test]
    fn test_notify_config_default() {
        let config = NotifyConfig::default();
        assert!(config.webhook.is_none());
        assert!(!config.paused_file);
        assert_eq!(config.timeout_secs, 30);
    }

    #[test]
    fn test_check_paused_file() {
        let temp = tempfile::TempDir::new().unwrap();
        assert!(!check_paused_file(temp.path()));

        std::fs::write(temp.path().join("PAUSED"), "test").unwrap();
        assert!(check_paused_file(temp.path()));
    }
}
