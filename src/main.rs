mod checkpoint;
mod cli;
mod config;
mod harness;
mod install;
mod logging;
mod loop_controller;
mod monitor;
mod notify;
mod process;
mod provider;
mod task;
mod tmux;
mod usage;

use anyhow::{bail, Context, Result};
use clap::Parser;
use dialoguer::{theme::ColorfulTheme, Select};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{info, trace};

use checkpoint::CheckpointManager;
use cli::{Cli, Commands};
use config::Config;
use harness::{Harness, Runner};
use loop_controller::{LoopConfig, LoopController};

fn check_harness_available(harness: &Harness) -> Result<()> {
    let cmd = harness.exec_command();
    which::which(cmd).with_context(|| {
        format!(
            "'{}' not found in PATH. Please install it first.",
            harness.command_name()
        )
    })?;
    Ok(())
}

fn resolve_prompt(task: &str) -> Result<String> {
    let path = std::path::Path::new(task);
    if path.exists() && path.is_file() {
        std::fs::read_to_string(path).with_context(|| format!("Failed to read task file: {}", task))
    } else {
        Ok(task.to_string())
    }
}

fn print_example_config() {
    println!(
        r#"# Ralph configuration file (.ralphrc or .ralphrc.toml)
# Place in current directory or home directory

# Agent harness: codex, claude, pi, gemini
harness = "codex"

# Model to use (optional, defaults vary by harness)
# model = "gpt-5.2-codex"

# Default task file
task = "TASK.md"

# Number of iterations (number or "inf")
iterations = "1"

# Enable dangerous mode (skip permissions)
dangerous = true

# Reasoning effort for codex
reasoning_effort = "medium"

# Provider for pi harness (anthropic, openai, google, etc.)
# provider = "anthropic"

# Usage limits
# usage_limit_daily = 80
# usage_limit_weekly = 90
# usage_check_interval = 5
# fallback_harness = "gemini"

# Tmux settings
# tmux = false
# tmux_session_prefix = "ralph"
# tmux_attach = false

# Monitor settings
# monitor_interval = "5m"
# monitor_harness = "claude"

# =====================
# NEW: Loop Mode Settings
# =====================

# [loop]
# enabled = true
# max_iterations = 200
# task_file = "fix_plan.md"
# validate_cmd = "./validate.sh"
# checkpoint_interval = 60

# [harnesses]
# primary = "codex"
# fallback = "claude"

# [limits]
# codex_daily = 80
# claude_daily = 90

# [pause]
# strategy = {{ type = "fallback", harness = "claude" }}

# [notify]
# webhook = "https://hooks.slack.com/..."
# paused_file = true

# [circuit_breaker]
# no_progress_threshold = 5
# same_error_threshold = 3
# cooldown_seconds = 300

# [hooks]
# after_each = "./validate.sh"
# after_each_on_fail = "continue"
"#
    );
}

fn list_harnesses() {
    println!("Available harnesses:");
    println!();
    println!("  codex   - OpenAI Codex CLI agent (default model: gpt-5.2-codex)");
    println!("  claude  - Anthropic Claude CLI agent (default model: claude-opus-4-5-20251101)");
    println!("  pi      - Pi CLI agent (default provider: anthropic, model: claude-opus-4-5)");
    println!("  gemini  - Google Gemini CLI agent (default model: auto-gemini-3)");
}

#[allow(clippy::too_many_arguments)]
fn apply_usage_limits(
    harness: &mut Harness,
    model: &mut String,
    provider: &mut String,
    runner: &mut Runner,
    usage_limit_daily: Option<u8>,
    usage_limit_weekly: Option<u8>,
    fallback_harness: Option<Harness>,
    using_fallback: &mut bool,
    dangerous: bool,
    reasoning: &str,
) -> Result<bool> {
    let check = usage::check_usage_limits(
        harness.command_name(),
        usage_limit_daily,
        usage_limit_weekly,
    );

    for warning in check.warnings {
        eprintln!("Usage check warning: {}", warning);
    }

    if !check.exceeded {
        return Ok(true);
    }

    let reason = check.reasons.join(", ");

    if let Some(fallback) = fallback_harness {
        if !*using_fallback && fallback != *harness {
            check_harness_available(&fallback)?;
            *harness = fallback;
            *model = harness.default_model().to_string();
            if *harness == Harness::Pi && provider.is_empty() {
                *provider = harness.default_provider().to_string();
            }
            *runner = Runner::new(
                *harness,
                model.clone(),
                dangerous,
                reasoning.to_string(),
                provider.clone(),
            );
            *using_fallback = true;
            eprintln!(
                "Usage limit reached ({}). Switching to fallback harness: {}",
                reason,
                harness.command_name()
            );
            return Ok(true);
        }
    }

    eprintln!("Usage limit reached ({}). Stopping.", reason);
    Ok(false)
}

/// Handle the Runs subcommand
fn handle_runs_command(
    _list: bool,
    show: Option<String>,
    cleanup: Option<usize>,
    json: bool,
) -> Result<()> {
    let manager = CheckpointManager::new(CheckpointManager::default_state_dir(), 0);

    if let Some(run_id) = show {
        // Show specific run
        match manager.load_by_run_id(&run_id)? {
            Some(state) => {
                if json {
                    println!("{}", serde_json::to_string_pretty(&state)?);
                } else {
                    println!("Run ID: {}", state.run_id);
                    println!("Short ID: {}", state.short_id());
                    println!(
                        "Harness: {}{}",
                        state.harness,
                        if state.using_fallback {
                            " (fallback)"
                        } else {
                            ""
                        }
                    );
                    println!("Model: {}", state.model);
                    println!(
                        "Status: {}",
                        if state.completed {
                            "completed"
                        } else {
                            "in progress"
                        }
                    );
                    println!(
                        "Iteration: {}{}",
                        state.current_iteration,
                        state
                            .max_iterations
                            .map(|m| format!("/{}", m))
                            .unwrap_or_default()
                    );
                    if let Some(ref task) = state.current_task {
                        println!("Current task: {}", task);
                    }
                    println!("Tasks completed: {}", state.completed_tasks.len());
                    println!("Tokens used: {}", state.tokens_used);
                    println!("Estimated cost: ${:.4}", state.estimated_cost_usd);
                    println!("Started: {}", state.started_at);
                    println!("Last checkpoint: {}", state.last_checkpoint);
                    println!("Runtime: {}", state.runtime_human());
                    if let Some(ref reason) = state.exit_reason {
                        println!("Exit reason: {}", reason);
                    }
                }
            }
            None => {
                eprintln!("Run not found: {}", run_id);
            }
        }
        return Ok(());
    }

    if let Some(keep) = cleanup {
        let removed = manager.cleanup_old_runs(keep)?;
        println!("Removed {} old run files (kept last {})", removed, keep);
        return Ok(());
    }

    // List all runs (default if no specific action)
    let runs = manager.list_runs()?;
    if runs.is_empty() {
        println!("No saved runs found.");
        return Ok(());
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&runs)?);
    } else {
        println!("Saved runs ({}):", runs.len());
        println!();
        for run in &runs {
            let status = if run.completed { "done" } else { "incomplete" };
            let fallback = if run.using_fallback {
                " (fallback)"
            } else {
                ""
            };
            println!(
                "  {} [{} {}{}] iter:{}{} tasks:{} ${:.2} {}",
                run.short_id(),
                run.harness,
                status,
                fallback,
                run.current_iteration,
                run.max_iterations
                    .map(|m| format!("/{}", m))
                    .unwrap_or_default(),
                run.completed_tasks.len(),
                run.estimated_cost_usd,
                run.runtime_human()
            );
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Handle --log-file flag early (before logging init)
    if cli.log_file {
        println!("{}", logging::current_log_file().display());
        return Ok(());
    }

    // Initialize logging (keep guard alive for program duration)
    let _log_guard = logging::init_logging(cli.verbose, cli.log_stderr)?;

    info!(version = env!("CARGO_PKG_VERSION"), "ralph starting");
    trace!(?cli, "parsed CLI arguments");

    // Handle subcommands first
    if let Some(cmd) = &cli.command {
        match cmd {
            Commands::Providers { json } => {
                let providers = provider::detect_all_providers();
                if *json {
                    provider::print_providers_json(&providers)?;
                } else {
                    provider::print_providers(&providers);
                }
                return Ok(());
            }
            Commands::Usage { provider, json } => {
                let usage_list = if provider == "all" {
                    usage::get_all_usage()
                } else {
                    match usage::get_provider_usage(provider) {
                        Some(info) => vec![info],
                        None => {
                            eprintln!("Unknown provider: {}", provider);
                            return Ok(());
                        }
                    }
                };

                if *json {
                    usage::print_usage_json(&usage_list)?;
                } else {
                    usage::print_usage(&usage_list);
                }
                return Ok(());
            }
            Commands::Monitor {
                monitor_harness,
                monitor_model,
                monitor_interval,
                inner_harness,
                inner_model,
                task,
            } => {
                let config = Config::load()?.unwrap_or_default();

                let monitor_h = Harness::from_str(monitor_harness)?;
                let inner_h = Harness::from_str(inner_harness)?;

                check_harness_available(&monitor_h)?;
                check_harness_available(&inner_h)?;

                let monitor_m = monitor_model
                    .clone()
                    .unwrap_or_else(|| monitor_h.default_model().to_string());
                let inner_m = inner_model
                    .clone()
                    .unwrap_or_else(|| inner_h.default_model().to_string());

                let task_str = task
                    .clone()
                    .or(cli.task.clone())
                    .or(config.task.clone())
                    .unwrap_or_else(|| "TASK.md".to_string());
                let prompt = resolve_prompt(&task_str)?;

                let interval = monitor::parse_duration(monitor_interval)?;

                let dangerous = if cli.safe {
                    false
                } else {
                    config.dangerous.unwrap_or(cli.dangerous)
                };
                let reasoning = config.reasoning_effort.unwrap_or(cli.reasoning.clone());
                let provider_str = cli
                    .provider
                    .clone()
                    .or(config.provider.clone())
                    .unwrap_or_else(|| inner_h.default_provider().to_string());

                let monitor_config = monitor::MonitorConfig {
                    monitor_harness: monitor_h,
                    monitor_model: monitor_m,
                    monitor_interval: interval,
                    inner_harness: inner_h,
                    inner_model: inner_m,
                    task: prompt,
                    dangerous,
                    reasoning_effort: reasoning,
                    provider: provider_str,
                };

                return monitor::run_monitor(monitor_config).await;
            }
            Commands::Install { agent, all, list } => {
                install::run_install(agent.clone(), *all, *list)?;
                return Ok(());
            }
            Commands::Ps { json, all } => {
                let processes = process::list_processes();
                if *json {
                    process::print_processes_json(&processes)?;
                } else {
                    process::print_processes(&processes, *all);
                }
                return Ok(());
            }
            Commands::Kill {
                all,
                dir,
                harness,
                pid,
            } => {
                if let Some(pid) = pid {
                    if process::kill_process(*pid)? {
                        println!("Killed process {}", pid);
                    } else {
                        println!("Failed to kill process {}", pid);
                    }
                } else if *all {
                    let (killed, total) = process::kill_all_processes()?;
                    println!("Killed {}/{} tracked processes", killed, total);
                } else if let Some(dir) = dir {
                    let (killed, total) = process::kill_processes_by_dir(dir)?;
                    println!("Killed {}/{} processes in {}", killed, total, dir);
                } else if let Some(harness) = harness {
                    let (killed, total) = process::kill_processes_by_harness(harness)?;
                    println!("Killed {}/{} {} processes", killed, total, harness);
                } else {
                    // Default: kill processes in current directory
                    let cwd = std::env::current_dir()?.display().to_string();
                    let (killed, total) = process::kill_processes_by_dir(&cwd)?;
                    println!("Killed {}/{} processes in current directory", killed, total);
                }
                return Ok(());
            }
            Commands::Cleanup {
                discover,
                kill_orphans,
            } => {
                // Clean stale entries
                let cleaned = process::cleanup_registry()?;
                println!("Cleaned {} stale process entries", cleaned);

                if *discover {
                    let orphans = process::discover_orphan_processes()?;
                    if orphans.is_empty() {
                        println!("No orphaned agent processes found");
                    } else {
                        println!("Found {} orphaned processes:", orphans.len());
                        process::print_processes(&orphans, false);

                        if *kill_orphans {
                            for proc in &orphans {
                                if process::kill_process(proc.pid).unwrap_or(false) {
                                    println!("Killed orphan {}", proc.pid);
                                }
                            }
                        }
                    }
                }
                return Ok(());
            }
            Commands::Logs {
                lines,
                follow,
                path,
                clear,
            } => {
                let log_file = logging::current_log_file();

                if *path {
                    println!("{}", log_file.display());
                    return Ok(());
                }

                if *clear {
                    let log_dir = logging::log_dir();
                    if log_dir.exists() {
                        for entry in std::fs::read_dir(&log_dir)? {
                            let entry = entry?;
                            if entry
                                .path()
                                .extension()
                                .map(|e| e == "log")
                                .unwrap_or(false)
                                || entry.file_name().to_string_lossy().starts_with("ralph.")
                            {
                                std::fs::remove_file(entry.path())?;
                                println!("Removed: {}", entry.path().display());
                            }
                        }
                    }
                    println!("Logs cleared");
                    return Ok(());
                }

                if !log_file.exists() {
                    println!("No log file found at: {}", log_file.display());
                    println!("Run ralph with -v to enable logging");
                    return Ok(());
                }

                if *follow {
                    // Use tail -f for following
                    let status = std::process::Command::new("tail")
                        .args(["-f", &log_file.to_string_lossy()])
                        .status()?;
                    if !status.success() {
                        bail!("tail command failed");
                    }
                } else {
                    // Read and display last N lines
                    let content = std::fs::read_to_string(&log_file)?;
                    let all_lines: Vec<&str> = content.lines().collect();
                    let start = if *lines == 0 || *lines >= all_lines.len() {
                        0
                    } else {
                        all_lines.len() - *lines
                    };
                    for line in &all_lines[start..] {
                        println!("{}", line);
                    }
                }
                return Ok(());
            }
            Commands::Sessions { list, attach, json } => {
                if !tmux::tmux_available() {
                    bail!("tmux is not available. Please install tmux first.");
                }

                let sessions = tmux::list_ralph_sessions()?;
                if sessions.is_empty() {
                    println!("No ralph tmux sessions found.");
                    return Ok(());
                }

                if *json {
                    tmux::print_sessions_json(&sessions)?;
                    return Ok(());
                }

                if let Some(name) = attach {
                    eprintln!("Attaching to {} (detach with Ctrl+b d)...", name);
                    tmux::attach_session(name)?;
                    return Ok(());
                }

                if *list || !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
                    tmux::print_sessions(&sessions);
                    return Ok(());
                }

                let labels: Vec<String> = sessions.iter().map(tmux::session_label).collect();
                let selection = Select::with_theme(&ColorfulTheme::default())
                    .with_prompt("Select a ralph tmux session")
                    .items(&labels)
                    .default(0)
                    .interact_opt()?;

                if let Some(index) = selection {
                    let name = &sessions[index].name;
                    eprintln!("Attaching to {} (detach with Ctrl+b d)...", name);
                    tmux::attach_session(name)?;
                }

                return Ok(());
            }
            Commands::Runs {
                list,
                show,
                cleanup,
                json,
            } => {
                return handle_runs_command(*list, show.clone(), *cleanup, *json);
            }
        }
    }

    if cli.list_harnesses {
        list_harnesses();
        return Ok(());
    }

    if cli.init {
        print_example_config();
        return Ok(());
    }

    let config = Config::load()?.unwrap_or_default();

    // Determine harness from config priority
    let harness_str = cli
        .harness
        .clone()
        .or_else(|| config.effective_harness().map(|s| s.to_string()))
        .unwrap_or_else(|| "codex".to_string());
    let harness = Harness::from_str(&harness_str)?;

    check_harness_available(&harness)?;

    let model = cli
        .model
        .clone()
        .or_else(|| config.harnesses.primary_model.clone())
        .or(config.model.clone())
        .unwrap_or_else(|| harness.default_model().to_string());

    let dangerous = if cli.safe {
        false
    } else {
        config.dangerous.unwrap_or(cli.dangerous)
    };

    let reasoning = config
        .reasoning_effort
        .clone()
        .unwrap_or(cli.reasoning.clone());

    let provider_str = cli
        .provider
        .clone()
        .or(config.provider.clone())
        .unwrap_or_else(|| harness.default_provider().to_string());

    // Get usage limits with per-harness override support
    let usage_limit_daily = cli
        .usage_limit_daily
        .or_else(|| config.effective_daily_limit(&harness_str));
    let usage_limit_weekly = cli
        .usage_limit_weekly
        .or_else(|| config.effective_weekly_limit(&harness_str));
    let usage_check_interval = cli.usage_check_interval.or(config.usage_check_interval);

    let fallback_harness_str = cli
        .fallback_harness
        .clone()
        .or_else(|| config.effective_fallback().map(|s| s.to_string()));
    let fallback_harness = match &fallback_harness_str {
        Some(name) => Some(Harness::from_str(name)?),
        None => None,
    };

    let task = cli
        .task
        .clone()
        .or(config.task.clone())
        .unwrap_or_else(|| "TASK.md".to_string());

    let prompt = resolve_prompt(&task)?;

    let iterations = config.iterations.clone().unwrap_or(cli.iterations.clone());

    // Check if loop mode is enabled
    let loop_mode = cli.loop_mode || config.loop_enabled();

    // Determine if we should use tmux
    let use_tmux = if cli.no_tmux {
        false
    } else if cli.tmux {
        true
    } else {
        config.tmux.unwrap_or(false)
    };

    let tmux_attach = cli.tmux_attach || config.tmux_attach.unwrap_or(false);
    let tmux_prefix = config
        .tmux_session_prefix
        .clone()
        .unwrap_or_else(|| "ralph".to_string());

    // If using tmux, start in tmux session
    if use_tmux && tmux::tmux_available() {
        let base_session_name = cli
            .tmux_session
            .clone()
            .unwrap_or_else(|| tmux::generate_session_name(&tmux_prefix, harness.command_name()));
        let session_name = tmux::unique_session_name(&base_session_name);
        if session_name != base_session_name {
            eprintln!(
                "Session name '{}' already exists, using '{}'",
                base_session_name, session_name
            );
        }

        // Build args to pass to ralph (running without tmux inside)
        let mut args = vec![
            "-H".to_string(),
            harness_str.clone(),
            "-m".to_string(),
            model.clone(),
            "-n".to_string(),
            iterations.clone(),
            "--reasoning".to_string(),
            reasoning.clone(),
        ];

        if dangerous {
            args.push("--dangerous".to_string());
        } else {
            args.push("--safe".to_string());
        }

        if !provider_str.is_empty() {
            args.push("--provider".to_string());
            args.push(provider_str.clone());
        }

        if let Some(limit) = usage_limit_daily {
            args.push("--usage-limit-daily".to_string());
            args.push(limit.to_string());
        }

        if let Some(limit) = usage_limit_weekly {
            args.push("--usage-limit-weekly".to_string());
            args.push(limit.to_string());
        }

        if let Some(interval) = usage_check_interval {
            args.push("--usage-check-interval".to_string());
            args.push(interval.to_string());
        }

        if let Some(ref name) = fallback_harness_str {
            args.push("--fallback-harness".to_string());
            args.push(name.clone());
        }

        // Loop mode args
        if loop_mode {
            args.push("--loop-mode".to_string());
        }
        if let Some(max) = cli.max_iter.or(config.loop_config.max_iterations) {
            args.push("--max-iter".to_string());
            args.push(max.to_string());
        }
        if let Some(ref file) = cli
            .task_file
            .clone()
            .or(config.loop_config.task_file.clone())
        {
            args.push("--task-file".to_string());
            args.push(file.clone());
        }

        args.push("--no-tmux".to_string()); // Prevent nesting
        args.push(task);

        return tmux::start_in_tmux(&session_name, "ralph", &args, tmux_attach);
    }

    // Set up Ctrl+C handler
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        eprintln!("\nInterrupted. Cleaning up tracked processes...");
        r.store(false, Ordering::SeqCst);

        // Kill all tracked processes before exiting
        if let Ok((killed, total)) = process::kill_all_processes() {
            if total > 0 {
                eprintln!("Killed {}/{} tracked processes", killed, total);
            }
        }

        std::process::exit(130);
    })?;

    // Run in loop mode or simple mode
    if loop_mode {
        // Use the new loop controller
        let runner = Runner::new(
            harness,
            model.clone(),
            dangerous,
            reasoning.clone(),
            provider_str.clone(),
        );

        let loop_config = LoopConfig {
            max_iterations: cli.max_iter.or(config.effective_max_iterations()),
            task_file: cli
                .task_file
                .clone()
                .or(config.loop_config.task_file.clone())
                .map(PathBuf::from),
            validate_cmd: cli
                .validate_cmd
                .clone()
                .or(config.loop_config.validate_cmd.clone()),
            checkpoint_interval: cli
                .checkpoint_interval
                .unwrap_or_else(|| config.effective_checkpoint_interval()),
            enabled: true,
            resume: cli.resume.clone().flatten(),
            task_config: config.task_config.clone(),
            pause_strategy: config.effective_pause_strategy(),
            notify_config: config.notify.clone(),
        };

        let mut controller = LoopController::new(loop_config, runner, running.clone())?;

        // Set fallback harness if configured
        if let Some(fallback) = fallback_harness {
            let fallback_model = config
                .harnesses
                .fallback_model
                .clone()
                .unwrap_or_else(|| fallback.default_model().to_string());
            controller = controller.with_fallback(fallback, fallback_model);
        }

        // Set usage limits
        controller = controller.with_usage_limits(
            usage_limit_daily,
            usage_limit_weekly,
            usage_check_interval.unwrap_or(5),
        );

        // Run the loop
        let result = controller.run(&prompt).await?;

        info!(
            run_id = %result.run_id,
            iterations = result.iterations,
            tasks_completed = result.tasks_completed,
            exit_reason = %result.exit_reason,
            "loop completed"
        );

        return Ok(());
    }

    // Run directly (simple mode, no loop controller)
    let mut current_harness = harness;
    let mut current_model = model;
    let mut current_provider = provider_str;
    let mut runner = Runner::new(
        current_harness,
        current_model.clone(),
        dangerous,
        reasoning.clone(),
        current_provider.clone(),
    );

    let mut using_fallback = false;
    let should_continue = apply_usage_limits(
        &mut current_harness,
        &mut current_model,
        &mut current_provider,
        &mut runner,
        usage_limit_daily,
        usage_limit_weekly,
        fallback_harness,
        &mut using_fallback,
        dangerous,
        &reasoning,
    )?;
    if !should_continue {
        return Ok(());
    }

    if iterations == "inf" {
        let interval = usage_check_interval.unwrap_or(0);
        let mut iteration = 0u32;
        while running.load(Ordering::SeqCst) {
            runner.run(&prompt).await?;
            iteration = iteration.saturating_add(1);
            if interval > 0 && iteration.is_multiple_of(interval) {
                let should_continue = apply_usage_limits(
                    &mut current_harness,
                    &mut current_model,
                    &mut current_provider,
                    &mut runner,
                    usage_limit_daily,
                    usage_limit_weekly,
                    fallback_harness,
                    &mut using_fallback,
                    dangerous,
                    &reasoning,
                )?;
                if !should_continue {
                    break;
                }
            }
        }
    } else {
        let count: u32 = iterations
            .parse()
            .with_context(|| format!("Invalid iteration count: {}", iterations))?;
        if count == 0 {
            bail!("Iteration count must be at least 1");
        }
        for i in 0..count {
            if !running.load(Ordering::SeqCst) {
                break;
            }
            if count > 1 {
                eprintln!("--- Iteration {}/{} ---", i + 1, count);
            }
            runner.run(&prompt).await?;
            if let Some(interval) = usage_check_interval {
                if interval > 0 && (i + 1) % interval == 0 {
                    let should_continue = apply_usage_limits(
                        &mut current_harness,
                        &mut current_model,
                        &mut current_provider,
                        &mut runner,
                        usage_limit_daily,
                        usage_limit_weekly,
                        fallback_harness,
                        &mut using_fallback,
                        dangerous,
                        &reasoning,
                    )?;
                    if !should_continue {
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
