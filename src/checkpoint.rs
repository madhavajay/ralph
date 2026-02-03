//! Checkpoint and resume functionality for long-running loops
//!
//! Saves loop state to `.ralph/loop_state.json` for resuming after interruptions.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// State of a running loop, persisted for resume capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopState {
    /// Unique identifier for this run
    pub run_id: String,
    /// Current task ID being processed
    pub current_task: Option<String>,
    /// Current step within the task (for multi-step tasks)
    pub current_step: u32,
    /// Current iteration number
    pub current_iteration: u32,
    /// Maximum iterations configured
    pub max_iterations: Option<u32>,
    /// List of completed task IDs
    pub completed_tasks: Vec<String>,
    /// Total tokens used across all iterations
    pub tokens_used: u64,
    /// Estimated cost in USD
    pub estimated_cost_usd: f64,
    /// Current harness being used
    pub harness: String,
    /// Current model being used
    pub model: String,
    /// Whether we're using the fallback harness
    pub using_fallback: bool,
    /// Task file path
    pub task_file: Option<String>,
    /// When the run started
    pub started_at: DateTime<Utc>,
    /// When the last checkpoint was saved
    pub last_checkpoint: DateTime<Utc>,
    /// Exit reason if the loop exited
    pub exit_reason: Option<String>,
    /// Whether the run completed successfully
    pub completed: bool,
}

impl LoopState {
    /// Create a new loop state
    pub fn new(
        harness: &str,
        model: &str,
        max_iterations: Option<u32>,
        task_file: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            run_id: Uuid::new_v4().to_string(),
            current_task: None,
            current_step: 0,
            current_iteration: 0,
            max_iterations,
            completed_tasks: Vec::new(),
            tokens_used: 0,
            estimated_cost_usd: 0.0,
            harness: harness.to_string(),
            model: model.to_string(),
            using_fallback: false,
            task_file,
            started_at: now,
            last_checkpoint: now,
            exit_reason: None,
            completed: false,
        }
    }

    /// Get the short run ID (first 8 characters)
    pub fn short_id(&self) -> &str {
        &self.run_id[..8.min(self.run_id.len())]
    }

    /// Update the checkpoint timestamp
    pub fn touch(&mut self) {
        self.last_checkpoint = Utc::now();
    }

    /// Mark the current task as started
    pub fn start_task(&mut self, task_id: &str) {
        self.current_task = Some(task_id.to_string());
        self.current_step = 0;
        self.touch();
    }

    /// Mark the current task as completed
    pub fn complete_task(&mut self, task_id: &str) {
        if !self.completed_tasks.contains(&task_id.to_string()) {
            self.completed_tasks.push(task_id.to_string());
        }
        if self.current_task.as_deref() == Some(task_id) {
            self.current_task = None;
        }
        self.current_step = 0;
        self.touch();
    }

    /// Increment the iteration counter
    pub fn next_iteration(&mut self) {
        self.current_iteration = self.current_iteration.saturating_add(1);
        self.touch();
    }

    /// Add token usage
    pub fn add_tokens(&mut self, tokens: u64, cost: f64) {
        self.tokens_used = self.tokens_used.saturating_add(tokens);
        self.estimated_cost_usd += cost;
        self.touch();
    }

    /// Switch to fallback harness
    pub fn switch_harness(&mut self, harness: &str, model: &str) {
        self.harness = harness.to_string();
        self.model = model.to_string();
        self.using_fallback = true;
        self.touch();
    }

    /// Mark the run as completed
    pub fn mark_completed(&mut self, reason: Option<&str>) {
        self.completed = true;
        self.exit_reason = reason.map(|s| s.to_string());
        self.touch();
    }

    /// Get runtime duration
    pub fn runtime(&self) -> chrono::Duration {
        self.last_checkpoint - self.started_at
    }

    /// Get human-readable runtime
    pub fn runtime_human(&self) -> String {
        let duration = self.runtime();
        let hours = duration.num_hours();
        let minutes = duration.num_minutes() % 60;
        let seconds = duration.num_seconds() % 60;

        if hours > 0 {
            format!("{}h {}m", hours, minutes)
        } else if minutes > 0 {
            format!("{}m {}s", minutes, seconds)
        } else {
            format!("{}s", seconds)
        }
    }
}

/// Checkpoint manager for saving and loading loop state
pub struct CheckpointManager {
    /// Directory for state files (usually .ralph/)
    state_dir: PathBuf,
    /// Current checkpoint interval in seconds
    checkpoint_interval: u64,
    /// Last time we saved a checkpoint
    last_save: Option<std::time::Instant>,
}

impl CheckpointManager {
    /// Create a new checkpoint manager
    pub fn new(state_dir: PathBuf, checkpoint_interval: u64) -> Self {
        Self {
            state_dir,
            checkpoint_interval,
            last_save: None,
        }
    }

    /// Get the default state directory (.ralph/ in current directory)
    pub fn default_state_dir() -> PathBuf {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".ralph")
    }

    /// Get the state file path
    fn state_file(&self) -> PathBuf {
        self.state_dir.join("loop_state.json")
    }

    /// Get the state file path for a specific run
    fn state_file_for_run(&self, run_id: &str) -> PathBuf {
        self.state_dir.join(format!("loop_state_{}.json", run_id))
    }

    /// Ensure the state directory exists
    fn ensure_dir(&self) -> Result<()> {
        if !self.state_dir.exists() {
            std::fs::create_dir_all(&self.state_dir).with_context(|| {
                format!(
                    "Failed to create state directory: {}",
                    self.state_dir.display()
                )
            })?;
        }
        Ok(())
    }

    /// Save checkpoint (respects interval)
    pub fn save_if_needed(&mut self, state: &LoopState) -> Result<bool> {
        if self.checkpoint_interval == 0 {
            return Ok(false);
        }

        let now = std::time::Instant::now();
        let should_save = self
            .last_save
            .map(|last| now.duration_since(last).as_secs() >= self.checkpoint_interval)
            .unwrap_or(true);

        if should_save {
            self.save(state)?;
            self.last_save = Some(now);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Force save checkpoint
    pub fn save(&self, state: &LoopState) -> Result<()> {
        self.ensure_dir()?;

        let json = serde_json::to_string_pretty(state).context("Failed to serialize loop state")?;

        // Write to current state file
        std::fs::write(self.state_file(), &json).with_context(|| {
            format!(
                "Failed to write state file: {}",
                self.state_file().display()
            )
        })?;

        // Also write to run-specific file for history
        std::fs::write(self.state_file_for_run(&state.run_id), &json).ok();

        Ok(())
    }

    /// Load the most recent checkpoint
    pub fn load_latest(&self) -> Result<Option<LoopState>> {
        let state_file = self.state_file();
        if !state_file.exists() {
            return Ok(None);
        }

        self.load_from_file(&state_file)
    }

    /// Load checkpoint by run ID
    pub fn load_by_run_id(&self, run_id: &str) -> Result<Option<LoopState>> {
        // First try the run-specific file
        let run_file = self.state_file_for_run(run_id);
        if run_file.exists() {
            return self.load_from_file(&run_file);
        }

        // Fall back to current state file and check if it matches
        if let Some(state) = self.load_latest()? {
            if state.run_id.starts_with(run_id) || state.short_id() == run_id {
                return Ok(Some(state));
            }
        }

        Ok(None)
    }

    /// Load from a specific file
    fn load_from_file(&self, path: &Path) -> Result<Option<LoopState>> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read state file: {}", path.display()))?;

        match serde_json::from_str(&content) {
            Ok(state) => Ok(Some(state)),
            Err(e) => {
                tracing::warn!("Failed to parse state file, starting fresh: {}", e);
                Ok(None)
            }
        }
    }

    /// List all saved runs
    pub fn list_runs(&self) -> Result<Vec<LoopState>> {
        let mut runs = Vec::new();

        if !self.state_dir.exists() {
            return Ok(runs);
        }

        for entry in std::fs::read_dir(&self.state_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("loop_state_") && n.ends_with(".json"))
                .unwrap_or(false)
            {
                if let Ok(Some(state)) = self.load_from_file(&path) {
                    runs.push(state);
                }
            }
        }

        // Sort by start time, most recent first
        runs.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        Ok(runs)
    }

    /// Clear current state (after successful completion)
    #[allow(dead_code)]
    pub fn clear_current(&self) -> Result<()> {
        let state_file = self.state_file();
        if state_file.exists() {
            std::fs::remove_file(&state_file).with_context(|| {
                format!("Failed to remove state file: {}", state_file.display())
            })?;
        }
        Ok(())
    }

    /// Clean up old run files (keep only last N)
    pub fn cleanup_old_runs(&self, keep: usize) -> Result<usize> {
        let runs = self.list_runs()?;
        let mut removed = 0;

        for run in runs.iter().skip(keep) {
            let path = self.state_file_for_run(&run.run_id);
            if path.exists() && std::fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }

        Ok(removed)
    }
}

/// Check if there's a resumable run
#[allow(dead_code)]
pub fn has_resumable_run() -> bool {
    let manager = CheckpointManager::new(CheckpointManager::default_state_dir(), 0);
    manager
        .load_latest()
        .ok()
        .flatten()
        .map(|s| !s.completed)
        .unwrap_or(false)
}

/// Get the most recent incomplete run
#[allow(dead_code)]
pub fn get_resumable_run() -> Option<LoopState> {
    let manager = CheckpointManager::new(CheckpointManager::default_state_dir(), 0);
    manager
        .load_latest()
        .ok()
        .flatten()
        .filter(|s| !s.completed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_manager() -> (CheckpointManager, TempDir) {
        let temp = TempDir::new().unwrap();
        let manager = CheckpointManager::new(temp.path().to_path_buf(), 60);
        (manager, temp)
    }

    #[test]
    fn test_loop_state_new() {
        let state = LoopState::new(
            "codex",
            "gpt-5.2-codex",
            Some(200),
            Some("fix_plan.md".to_string()),
        );
        assert!(!state.run_id.is_empty());
        assert_eq!(state.harness, "codex");
        assert_eq!(state.model, "gpt-5.2-codex");
        assert_eq!(state.max_iterations, Some(200));
        assert_eq!(state.current_iteration, 0);
        assert!(!state.completed);
    }

    #[test]
    fn test_loop_state_task_lifecycle() {
        let mut state = LoopState::new("codex", "gpt-5.2-codex", None, None);

        state.start_task("A.1");
        assert_eq!(state.current_task, Some("A.1".to_string()));

        state.complete_task("A.1");
        assert_eq!(state.current_task, None);
        assert!(state.completed_tasks.contains(&"A.1".to_string()));
    }

    #[test]
    fn test_loop_state_iterations() {
        let mut state = LoopState::new("codex", "gpt-5.2-codex", Some(10), None);

        for _ in 0..5 {
            state.next_iteration();
        }
        assert_eq!(state.current_iteration, 5);
    }

    #[test]
    fn test_loop_state_tokens() {
        let mut state = LoopState::new("codex", "gpt-5.2-codex", None, None);

        state.add_tokens(1000, 0.01);
        state.add_tokens(2000, 0.02);

        assert_eq!(state.tokens_used, 3000);
        assert!((state.estimated_cost_usd - 0.03).abs() < 0.0001);
    }

    #[test]
    fn test_loop_state_harness_switch() {
        let mut state = LoopState::new("codex", "gpt-5.2-codex", None, None);

        state.switch_harness("claude", "claude-opus-4-5");
        assert_eq!(state.harness, "claude");
        assert_eq!(state.model, "claude-opus-4-5");
        assert!(state.using_fallback);
    }

    #[test]
    fn test_checkpoint_save_load() {
        let (manager, _temp) = test_manager();

        let mut state = LoopState::new("codex", "gpt-5.2-codex", Some(100), None);
        state.start_task("A.1");
        state.next_iteration();

        manager.save(&state).unwrap();

        let loaded = manager.load_latest().unwrap().unwrap();
        assert_eq!(loaded.run_id, state.run_id);
        assert_eq!(loaded.harness, "codex");
        assert_eq!(loaded.current_task, Some("A.1".to_string()));
        assert_eq!(loaded.current_iteration, 1);
    }

    #[test]
    fn test_checkpoint_load_by_run_id() {
        let (manager, _temp) = test_manager();

        let state = LoopState::new("codex", "gpt-5.2-codex", None, None);
        let run_id = state.run_id.clone();
        let short_id = state.short_id().to_string();

        manager.save(&state).unwrap();

        // Load by full ID
        let loaded = manager.load_by_run_id(&run_id).unwrap().unwrap();
        assert_eq!(loaded.run_id, run_id);

        // Load by short ID
        let loaded = manager.load_by_run_id(&short_id).unwrap().unwrap();
        assert_eq!(loaded.run_id, run_id);
    }

    #[test]
    fn test_checkpoint_list_runs() {
        let (manager, _temp) = test_manager();

        // Create multiple runs
        for i in 0..3 {
            let mut state = LoopState::new("codex", "gpt-5.2-codex", None, None);
            state.current_iteration = i;
            manager.save(&state).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let runs = manager.list_runs().unwrap();
        assert!(!runs.is_empty()); // At least the run-specific files
    }

    #[test]
    fn test_checkpoint_clear() {
        let (manager, _temp) = test_manager();

        let state = LoopState::new("codex", "gpt-5.2-codex", None, None);
        manager.save(&state).unwrap();

        assert!(manager.load_latest().unwrap().is_some());

        manager.clear_current().unwrap();

        assert!(manager.load_latest().unwrap().is_none());
    }

    #[test]
    fn test_runtime_human() {
        let state = LoopState::new("codex", "gpt-5.2-codex", None, None);
        // The runtime should be very short
        let runtime = state.runtime_human();
        assert!(runtime.contains('s') || runtime.contains('m'));
    }

    #[test]
    fn test_short_id() {
        let state = LoopState::new("codex", "gpt-5.2-codex", None, None);
        let short = state.short_id();
        assert_eq!(short.len(), 8);
    }
}
