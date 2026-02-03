//! Loop controller for managing iterative agent execution
//!
//! Provides a unified loop execution with task management, checkpointing,
//! usage limit handling, and harness switching.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::checkpoint::{CheckpointManager, LoopState};
use crate::harness::{Harness, Runner};
use crate::notify::{
    Notifier, NotifyConfig, NotifyEvent, PauseAction, PauseContext, PauseStrategy,
};
use crate::task::{self, TaskConfig};
use crate::usage;

/// Configuration for loop execution
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct LoopConfig {
    /// Maximum number of iterations (None for infinite)
    pub max_iterations: Option<u32>,
    /// Task file path (e.g., "fix_plan.md")
    pub task_file: Option<PathBuf>,
    /// Command to run for validation after each iteration
    pub validate_cmd: Option<String>,
    /// Checkpoint interval in seconds (0 to disable)
    pub checkpoint_interval: u64,
    /// Whether to enable loop mode
    pub enabled: bool,
    /// Whether to resume from a previous run
    pub resume: Option<String>,
    /// Task configuration
    pub task_config: TaskConfig,
    /// Pause strategy
    pub pause_strategy: PauseStrategy,
    /// Notification configuration
    pub notify_config: NotifyConfig,
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: Some(200),
            task_file: None,
            validate_cmd: None,
            checkpoint_interval: 60,
            enabled: false,
            resume: None,
            task_config: TaskConfig::default(),
            pause_strategy: PauseStrategy::Exit,
            notify_config: NotifyConfig::default(),
        }
    }
}

/// Results from a single iteration
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct IterationResult {
    /// Whether the iteration succeeded
    pub success: bool,
    /// Exit code from the harness
    pub exit_code: Option<i32>,
    /// Tokens used in this iteration
    pub tokens_used: u64,
    /// Estimated cost for this iteration
    pub estimated_cost_usd: f64,
    /// Files modified during iteration
    pub files_modified: Vec<String>,
    /// Task ID if running in task mode
    pub task_id: Option<String>,
    /// Whether the task was completed
    pub task_completed: bool,
    /// Error message if failed
    pub error: Option<String>,
}

/// Overall loop execution result
#[derive(Debug)]
#[allow(dead_code)]
pub struct LoopResult {
    /// Total iterations executed
    pub iterations: u32,
    /// Tasks completed
    pub tasks_completed: usize,
    /// Total tokens used
    pub total_tokens: u64,
    /// Total estimated cost
    pub total_cost_usd: f64,
    /// Runtime in human-readable format
    pub runtime_human: String,
    /// Exit reason
    pub exit_reason: ExitReason,
    /// Run ID
    pub run_id: String,
}

/// Reason for loop exit
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ExitReason {
    /// All iterations completed
    MaxIterationsReached,
    /// All tasks completed
    AllTasksCompleted,
    /// Usage limit exceeded
    UsageLimitExceeded { reason: String },
    /// User interrupted (Ctrl+C)
    UserInterrupted,
    /// Validation command failed
    ValidationFailed { command: String, exit_code: i32 },
    /// Error occurred
    Error { message: String },
    /// Circuit breaker triggered
    CircuitBreakerTripped { reason: String },
}

impl std::fmt::Display for ExitReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MaxIterationsReached => write!(f, "max iterations reached"),
            Self::AllTasksCompleted => write!(f, "all tasks completed"),
            Self::UsageLimitExceeded { reason } => write!(f, "usage limit exceeded: {}", reason),
            Self::UserInterrupted => write!(f, "user interrupted"),
            Self::ValidationFailed { command, exit_code } => {
                write!(f, "validation failed: {} (exit {})", command, exit_code)
            }
            Self::Error { message } => write!(f, "error: {}", message),
            Self::CircuitBreakerTripped { reason } => write!(f, "circuit breaker: {}", reason),
        }
    }
}

/// Loop controller manages the execution loop
pub struct LoopController {
    config: LoopConfig,
    runner: Runner,
    checkpoint_manager: CheckpointManager,
    notifier: Notifier,
    state: LoopState,
    running: Arc<AtomicBool>,
    /// Fallback harness configuration
    fallback_harness: Option<Harness>,
    fallback_model: Option<String>,
    /// Usage limits
    usage_limit_daily: Option<u8>,
    usage_limit_weekly: Option<u8>,
    usage_check_interval: u32,
}

impl LoopController {
    /// Create a new loop controller
    pub fn new(config: LoopConfig, runner: Runner, running: Arc<AtomicBool>) -> Result<Self> {
        let state_dir = CheckpointManager::default_state_dir();
        let checkpoint_manager =
            CheckpointManager::new(state_dir.clone(), config.checkpoint_interval);
        let notifier = Notifier::new(config.notify_config.clone(), state_dir);

        // Load or create state
        let state = if let Some(ref resume_id) = config.resume {
            let loaded = if resume_id.is_empty() {
                checkpoint_manager.load_latest()?
            } else {
                checkpoint_manager.load_by_run_id(resume_id)?
            };

            match loaded {
                Some(s) if !s.completed => {
                    info!(run_id = %s.short_id(), "Resuming previous run");
                    s
                }
                Some(s) => {
                    warn!(run_id = %s.short_id(), "Previous run already completed, starting new");
                    LoopState::new(
                        runner.harness.command_name(),
                        &runner.model,
                        config.max_iterations,
                        config.task_file.as_ref().map(|p| p.display().to_string()),
                    )
                }
                None => {
                    warn!("No previous run found, starting new");
                    LoopState::new(
                        runner.harness.command_name(),
                        &runner.model,
                        config.max_iterations,
                        config.task_file.as_ref().map(|p| p.display().to_string()),
                    )
                }
            }
        } else {
            LoopState::new(
                runner.harness.command_name(),
                &runner.model,
                config.max_iterations,
                config.task_file.as_ref().map(|p| p.display().to_string()),
            )
        };

        Ok(Self {
            config,
            runner,
            checkpoint_manager,
            notifier,
            state,
            running,
            fallback_harness: None,
            fallback_model: None,
            usage_limit_daily: None,
            usage_limit_weekly: None,
            usage_check_interval: 0,
        })
    }

    /// Set fallback harness
    pub fn with_fallback(mut self, harness: Harness, model: String) -> Self {
        self.fallback_harness = Some(harness);
        self.fallback_model = Some(model);
        self
    }

    /// Set usage limits
    pub fn with_usage_limits(
        mut self,
        daily: Option<u8>,
        weekly: Option<u8>,
        check_interval: u32,
    ) -> Self {
        self.usage_limit_daily = daily;
        self.usage_limit_weekly = weekly;
        self.usage_check_interval = check_interval;
        self
    }

    /// Run the loop
    pub async fn run(&mut self, base_prompt: &str) -> Result<LoopResult> {
        info!(
            run_id = %self.state.short_id(),
            max_iterations = ?self.config.max_iterations,
            task_file = ?self.config.task_file,
            "Starting loop"
        );

        // Clear any previous paused state
        self.notifier.clear_paused_file().ok();

        // Save initial checkpoint
        self.checkpoint_manager.save(&self.state)?;

        let exit_reason = self.run_loop(base_prompt).await;

        // Mark as completed
        self.state.mark_completed(Some(&exit_reason.to_string()));
        self.checkpoint_manager.save(&self.state)?;

        // Send completion notification
        let event = NotifyEvent::LoopCompleted {
            run_id: self.state.run_id.clone(),
            total_iterations: self.state.current_iteration,
            tasks_completed: self.state.completed_tasks.len(),
            total_tokens: self.state.tokens_used,
            estimated_cost_usd: self.state.estimated_cost_usd,
            runtime_human: self.state.runtime_human(),
        };
        self.notifier.notify(&event).await.ok();

        // Print summary
        self.print_summary(&exit_reason);

        Ok(LoopResult {
            iterations: self.state.current_iteration,
            tasks_completed: self.state.completed_tasks.len(),
            total_tokens: self.state.tokens_used,
            total_cost_usd: self.state.estimated_cost_usd,
            runtime_human: self.state.runtime_human(),
            exit_reason,
            run_id: self.state.run_id.clone(),
        })
    }

    /// Main loop execution
    async fn run_loop(&mut self, base_prompt: &str) -> ExitReason {
        loop {
            // Check if we should stop
            if !self.running.load(Ordering::SeqCst) {
                return ExitReason::UserInterrupted;
            }

            // Check max iterations
            if let Some(max) = self.config.max_iterations {
                if self.state.current_iteration >= max {
                    return ExitReason::MaxIterationsReached;
                }
            }

            // Check usage limits periodically
            if self.usage_check_interval > 0
                && self.state.current_iteration > 0
                && self
                    .state
                    .current_iteration
                    .is_multiple_of(self.usage_check_interval)
            {
                match self.check_and_handle_usage_limits().await {
                    Ok(true) => {} // Continue
                    Ok(false) => {
                        return ExitReason::UsageLimitExceeded {
                            reason: "limit reached and no fallback available".to_string(),
                        }
                    }
                    Err(e) => {
                        warn!("Error checking usage limits: {}", e);
                    }
                }
            }

            // Get the prompt for this iteration
            let prompt = match self.get_iteration_prompt(base_prompt).await {
                Ok(Some(p)) => p,
                Ok(None) => return ExitReason::AllTasksCompleted,
                Err(e) => {
                    error!("Error getting prompt: {}", e);
                    return ExitReason::Error {
                        message: e.to_string(),
                    };
                }
            };

            // Print iteration header
            self.print_iteration_header();

            // Run the iteration
            let result = self.run_iteration(&prompt).await;

            // Update state based on result
            self.state.next_iteration();
            if let Some(ref task_id) = result.task_id {
                if result.task_completed {
                    self.state.complete_task(task_id);
                }
            }
            self.state
                .add_tokens(result.tokens_used, result.estimated_cost_usd);

            // Save checkpoint
            if let Err(e) = self.checkpoint_manager.save_if_needed(&self.state) {
                warn!("Failed to save checkpoint: {}", e);
            }

            // Run validation if configured
            if let Some(ref cmd) = self.config.validate_cmd {
                match self.run_validation(cmd) {
                    Ok(true) => {}
                    Ok(false) => {
                        return ExitReason::ValidationFailed {
                            command: cmd.clone(),
                            exit_code: 1,
                        }
                    }
                    Err(e) => {
                        warn!("Validation error: {}", e);
                    }
                }
            }

            // Handle iteration failure
            if !result.success {
                if let Some(ref error) = result.error {
                    error!("Iteration failed: {}", error);
                }
                // Continue for now - circuit breaker would handle repeated failures
            }
        }
    }

    /// Get the prompt for the current iteration
    async fn get_iteration_prompt(&mut self, base_prompt: &str) -> Result<Option<String>> {
        // If we have a task file, get the next task
        if let Some(ref task_file) = self.config.task_file {
            if task_file.exists() {
                let next_task = task::get_next_task(task_file, &self.config.task_config)?;

                match next_task {
                    Some(task) => {
                        self.state.start_task(&task.id);
                        // Format prompt with task
                        let prompt = format!(
                            "{}\n\nCurrent task ({}): {}\n\nComplete this task and mark it done in the task file.",
                            base_prompt,
                            task.id,
                            task.content
                        );
                        return Ok(Some(prompt));
                    }
                    None => {
                        // No more tasks
                        return Ok(None);
                    }
                }
            }
        }

        // No task file, use base prompt
        Ok(Some(base_prompt.to_string()))
    }

    /// Run a single iteration
    async fn run_iteration(&mut self, prompt: &str) -> IterationResult {
        let mut result = IterationResult::default();

        match self.runner.run(prompt).await {
            Ok(()) => {
                result.success = true;
                // Note: tokens/cost would come from harness output parsing
                // For now, we don't have this data from the runner
            }
            Err(e) => {
                result.success = false;
                result.error = Some(e.to_string());
            }
        }

        // Check if current task was completed (by looking at task file)
        if let Some(ref task_file) = self.config.task_file {
            if let Some(ref task_id) = self.state.current_task {
                if let Ok(Some(task)) =
                    task::get_task_by_id(task_file, task_id, &self.config.task_config)
                {
                    result.task_id = Some(task_id.clone());
                    result.task_completed = task.completed;
                }
            }
        }

        result
    }

    /// Check usage limits and handle according to strategy
    async fn check_and_handle_usage_limits(&mut self) -> Result<bool> {
        let check = usage::check_usage_limits(
            self.runner.harness.command_name(),
            self.usage_limit_daily,
            self.usage_limit_weekly,
        );

        // Log warnings
        for warning in &check.warnings {
            warn!("Usage warning: {}", warning);
        }

        if !check.exceeded {
            return Ok(true);
        }

        let reason = check.reasons.join(", ");
        info!("Usage limit exceeded: {}", reason);

        // Execute pause strategy
        let context = PauseContext {
            harness: self.runner.harness.command_name(),
            limit_type: if self.usage_limit_daily.is_some() {
                "daily"
            } else {
                "weekly"
            },
            usage_percent: 0, // Would need actual value from usage check
            limit_percent: self.usage_limit_daily.unwrap_or(0),
        };

        let action = crate::notify::execute_pause_strategy(
            &self.config.pause_strategy,
            &self.notifier,
            context,
        )
        .await?;

        match action {
            PauseAction::Exit => Ok(false),
            PauseAction::Wait { seconds } => {
                info!("Waiting {} seconds before continuing", seconds);
                tokio::time::sleep(tokio::time::Duration::from_secs(seconds)).await;
                Ok(true)
            }
            PauseAction::SwitchHarness { harness } => {
                self.switch_to_fallback(&harness)?;
                Ok(true)
            }
        }
    }

    /// Switch to fallback harness
    fn switch_to_fallback(&mut self, harness_name: &str) -> Result<()> {
        let harness = Harness::from_str(harness_name)?;
        let model = self
            .fallback_model
            .clone()
            .unwrap_or_else(|| harness.default_model().to_string());

        info!(
            from = %self.runner.harness.command_name(),
            to = %harness_name,
            "Switching to fallback harness"
        );

        self.runner = Runner::new(
            harness,
            model.clone(),
            self.runner.dangerous,
            self.runner.reasoning_effort.clone(),
            self.runner.provider.clone(),
        );

        self.state.switch_harness(harness_name, &model);

        Ok(())
    }

    /// Run validation command
    fn run_validation(&self, cmd: &str) -> Result<bool> {
        debug!("Running validation: {}", cmd);

        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .context("Failed to run validation command")?;

        Ok(output.status.success())
    }

    /// Print iteration header
    fn print_iteration_header(&self) {
        let iter_str = if let Some(max) = self.config.max_iterations {
            format!("{}/{}", self.state.current_iteration + 1, max)
        } else {
            format!("{}", self.state.current_iteration + 1)
        };

        let task_str = self
            .state
            .current_task
            .as_ref()
            .map(|t| format!(" [{}]", t))
            .unwrap_or_default();

        eprintln!(
            "--- Iteration {}{} ({}) ---",
            iter_str,
            task_str,
            self.runner.harness.command_name()
        );
    }

    /// Print final summary
    fn print_summary(&self, exit_reason: &ExitReason) {
        eprintln!();
        eprintln!("=== Loop Summary ===");
        eprintln!("Run ID: {}", self.state.short_id());
        eprintln!("Exit reason: {}", exit_reason);
        eprintln!(
            "Iterations: {}{}",
            self.state.current_iteration,
            self.config
                .max_iterations
                .map(|m| format!("/{}", m))
                .unwrap_or_default()
        );
        if !self.state.completed_tasks.is_empty() {
            eprintln!("Tasks completed: {}", self.state.completed_tasks.len());
        }
        eprintln!("Tokens used: {}", self.state.tokens_used);
        if self.state.estimated_cost_usd > 0.0 {
            eprintln!("Estimated cost: ${:.4}", self.state.estimated_cost_usd);
        }
        eprintln!("Runtime: {}", self.state.runtime_human());
        if self.state.using_fallback {
            eprintln!("Harness: {} (fallback)", self.state.harness);
        }
        eprintln!();
    }
}

/// Parse iteration count from string
#[allow(dead_code)]
pub fn parse_iterations(s: &str) -> Option<u32> {
    if s == "inf" || s == "infinite" {
        None
    } else {
        s.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loop_config_default() {
        let config = LoopConfig::default();
        assert_eq!(config.max_iterations, Some(200));
        assert!(!config.enabled);
        assert_eq!(config.checkpoint_interval, 60);
    }

    #[test]
    fn test_parse_iterations() {
        assert_eq!(parse_iterations("100"), Some(100));
        assert_eq!(parse_iterations("inf"), None);
        assert_eq!(parse_iterations("infinite"), None);
        assert_eq!(parse_iterations("abc"), None);
    }

    #[test]
    fn test_exit_reason_display() {
        assert_eq!(
            ExitReason::MaxIterationsReached.to_string(),
            "max iterations reached"
        );
        assert_eq!(
            ExitReason::AllTasksCompleted.to_string(),
            "all tasks completed"
        );
        assert!(ExitReason::UsageLimitExceeded {
            reason: "daily".to_string()
        }
        .to_string()
        .contains("daily"));
    }

    #[test]
    fn test_iteration_result_default() {
        let result = IterationResult::default();
        assert!(!result.success);
        assert!(result.task_id.is_none());
        assert_eq!(result.tokens_used, 0);
    }
}
