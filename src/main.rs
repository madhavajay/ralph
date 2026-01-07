mod cli;
mod config;
mod harness;
mod monitor;
mod provider;
mod tmux;
mod usage;

use anyhow::{bail, Context, Result};
use clap::Parser;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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
# tmux = true
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
    println!("  gemini  - Google Gemini CLI agent (default model: gemini-2.5-pro)");
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

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
        // Check config, default to true if tmux is available
        config.tmux.unwrap_or_else(tmux::tmux_available)
    };

    let tmux_attach = cli.tmux_attach || config.tmux_attach.unwrap_or(false);
    let tmux_prefix = config
        .tmux_session_prefix
        .unwrap_or_else(|| "ralph".to_string());

    // If using tmux, start in tmux session
    if use_tmux && tmux::tmux_available() {
        let session_name = cli
            .tmux_session
            .unwrap_or_else(|| tmux::generate_session_name(&tmux_prefix, harness.command_name()));

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

        args.push("--no-tmux".to_string()); // Prevent nesting
        args.push(task);

        return tmux::start_in_tmux(&session_name, "ralph", &args, tmux_attach);
    }

    // Run directly (no tmux)
    let runner = Runner::new(harness, model, dangerous, reasoning, provider_str);

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        eprintln!("\nInterrupted. Cleaning up...");
        r.store(false, Ordering::SeqCst);
        std::process::exit(130);
    })?;

    if iterations == "inf" {
        while running.load(Ordering::SeqCst) {
            runner.run(&prompt).await?;
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
        }
    }

    Ok(())
}
