# Changelog

All notable changes to Ralph Rust will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

#### P0.1 - Built-in Loop Controller (`loop_controller.rs`)
- New `--loop-mode` flag to enable loop execution mode
- `--max-iter N` to set maximum iterations (default: 200, use "inf" for infinite)
- `--validate-cmd CMD` to run validation after each iteration
- `LoopController` struct managing iterative agent execution with:
  - Automatic task progression
  - Usage limit checking and handling
  - Harness switching on limits
  - Graceful Ctrl+C handling
- `ExitReason` enum for various loop exit scenarios (MaxIterationsReached, AllTasksCompleted, UsageLimitExceeded, UserInterrupted, ValidationFailed, Error, CircuitBreakerTripped)

#### P0.2 - Task File Parser (`task.rs`)
- `--task-file FILE` to specify task file (e.g., `fix_plan.md`)
- Tolerant parsing supporting multiple markdown checkbox formats:
  - `- [ ] A.1 task description` (standard)
  - `- [ ] **A.1** task description` (bold)
  - `- [ ] A.1 task (P0)` (with priority)
  - `- [x] A.1 completed task` (completed)
- Configurable regex pattern via `[task] pattern = "..."` in config
- Task filtering by prefix (`filter_prefix`) and priority (`filter_priority`)
- Functions: `parse_tasks()`, `get_next_task()`, `get_task_by_id()`, `mark_task_done()`, `mark_task_pending()`

#### P0.3 - Checkpoint/Resume (`checkpoint.rs`)
- `--resume [RUN_ID]` to resume a previous run (empty for latest)
- `--checkpoint-interval SECS` to set checkpoint save interval (default: 60)
- State persistence to `.ralph/loop_state.json` including:
  - Run ID, current task, iteration count
  - Completed tasks list
  - Token usage and estimated cost
  - Harness and model information
  - Start time and last checkpoint time
- `CheckpointManager` for save/load operations
- Run-specific state files for history (`.ralph/loop_state_{run_id}.json`)
- New `runs` subcommand:
  - `ralph runs list` - List all saved runs
  - `ralph runs show RUN_ID` - Show details of a specific run
  - `ralph runs cleanup [--keep N]` - Clean up old run files

#### P0.4 - Notification & Pause Strategies (`notify.rs`)
- `PauseStrategy` enum with configurable behaviors:
  - `Exit` - Exit immediately when limit reached
  - `Wait { until_reset }` - Wait for a period or until reset time
  - `Fallback { harness }` - Switch to fallback harness
  - `Notify { webhook, paused_file, then }` - Send notification then execute another strategy
- Webhook notifications via HTTP POST with JSON payload
- `PAUSED` file generation in `.ralph/` directory
- `NotifyEvent` types: UsageLimitReached, LoopPaused, LoopResumed, LoopCompleted, Error, HarnessSwitched, CircuitBreakerTriggered
- Configuration via `[notify]` and `[pause]` sections in `.ralphrc`

#### P0.5 - Dual Model Auto-Switching
- `[harnesses]` config section with `primary` and `fallback` harness settings
- `[limits]` config section for per-harness usage limits:
  - `codex_daily`, `codex_weekly`
  - `claude_daily`, `claude_weekly`
- Automatic switching from primary to fallback harness when usage limits exceeded
- `LoopController.with_fallback()` and `with_usage_limits()` builder methods

#### New Configuration Sections
- `[loop]` - Loop mode settings (max_iterations, task_file, validate_cmd, checkpoint_interval)
- `[harnesses]` - Primary and fallback harness configuration
- `[limits]` - Per-harness usage limits
- `[circuit_breaker]` - Circuit breaker thresholds (for P1)
- `[hooks]` - Validation hooks configuration (for P1)
- `[notify]` - Webhook and notification settings
- `[pause]` - Pause strategy configuration
- `[task]` - Task parsing configuration

#### New Dependencies
- `regex = "1.10"` - Task file pattern matching
- `uuid = { version = "1.10", features = ["v4"] }` - Run ID generation
- `reqwest = { version = "0.12", features = ["json", "blocking"] }` - Webhook notifications
- `chrono` with `serde` feature - Timestamp serialization

### Changed
- `config.rs` - Added `effective_*` methods for config priority resolution (CLI > env > config > defaults)
- `cli.rs` - Extended with new loop-related arguments and `runs` subcommand
- `main.rs` - Integrated all P0 modules with loop mode execution path

## [0.1.5] - Previous Release

Initial multi-harness support with Codex, Claude, Pi, and Gemini harnesses.
