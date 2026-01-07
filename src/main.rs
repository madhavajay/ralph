mod cli;
mod config;
mod harness;
mod install;
mod logging;
mod monitor;
mod process;
mod provider;
mod tmux;
mod usage;

use anyhow::{bail, Context, Result};
use clap::Parser;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{info, trace};

use cli::{Cli, Commands};
use config::Config;
use harness::{Harness, Runner};

fn check_harness_available(harness: &Harness) -> Result<()> {
    let cmd = harness.command_name();
    which::which(cmd)
        .with_context(|| format!("'{}' not found in PATH. Please install it first.", cmd))?;
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

    let harness_str = cli
        .harness
        .or(config.harness)
        .unwrap_or_else(|| "codex".to_string());
    let harness = Harness::from_str(&harness_str)?;

    check_harness_available(&harness)?;

    let model = cli
        .model
        .or(config.model)
        .unwrap_or_else(|| harness.default_model().to_string());

    let dangerous = if cli.safe {
        false
    } else {
        config.dangerous.unwrap_or(cli.dangerous)
    };

    let reasoning = config.reasoning_effort.unwrap_or(cli.reasoning);

    let provider_str = cli
        .provider
        .or(config.provider)
        .unwrap_or_else(|| harness.default_provider().to_string());

    let usage_limit_daily = cli.usage_limit_daily.or(config.usage_limit_daily);
    let usage_limit_weekly = cli.usage_limit_weekly.or(config.usage_limit_weekly);
    let usage_check_interval = cli.usage_check_interval.or(config.usage_check_interval);
    let fallback_harness_str = cli.fallback_harness.or(config.fallback_harness);
    let fallback_harness = match &fallback_harness_str {
        Some(name) => Some(Harness::from_str(name)?),
        None => None,
    };

    let task = cli
        .task
        .or(config.task)
        .unwrap_or_else(|| "TASK.md".to_string());

    let prompt = resolve_prompt(&task)?;

    let iterations = config.iterations.unwrap_or(cli.iterations);

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
        .unwrap_or_else(|| "ralph".to_string());

    // If using tmux, start in tmux session
    if use_tmux && tmux::tmux_available() {
        let base_session_name = cli
            .tmux_session
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
            harness_str,
            "-m".to_string(),
            model,
            "-n".to_string(),
            iterations,
            "--reasoning".to_string(),
            reasoning,
        ];

        if dangerous {
            args.push("--dangerous".to_string());
        } else {
            args.push("--safe".to_string());
        }

        if !provider_str.is_empty() {
            args.push("--provider".to_string());
            args.push(provider_str);
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

        if let Some(name) = fallback_harness_str {
            args.push("--fallback-harness".to_string());
            args.push(name);
        }

        args.push("--no-tmux".to_string()); // Prevent nesting
        args.push(task);

        return tmux::start_in_tmux(&session_name, "ralph", &args, tmux_attach);
    }

    // Run directly (no tmux)
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
