mod cli;
mod config;
mod harness;

use anyhow::{bail, Context, Result};
use clap::Parser;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use cli::Cli;
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
        std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read task file: {}", task))
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
"#
    );
}

fn list_harnesses() {
    println!("Available harnesses:");
    println!();
    println!("  codex   - OpenAI Codex CLI agent (default model: gpt-5.2-codex)");
    println!("  claude  - Anthropic Claude CLI agent (default model: claude-opus-4-5-20251101)");
    println!("  pi      - Pi CLI agent (default model: pi)");
    println!("  gemini  - Google Gemini CLI agent (default model: gemini-2.5-pro)");
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.list_harnesses {
        list_harnesses();
        return Ok(());
    }

    if cli.init {
        print_example_config();
        return Ok(());
    }

    let config = Config::load()?.unwrap_or_default();

    let harness_str = cli.harness
        .or(config.harness)
        .unwrap_or_else(|| "codex".to_string());
    let harness = Harness::from_str(&harness_str)?;

    check_harness_available(&harness)?;

    let model = cli.model
        .or(config.model)
        .unwrap_or_else(|| harness.default_model().to_string());

    let dangerous = if cli.safe {
        false
    } else {
        config.dangerous.unwrap_or(cli.dangerous)
    };

    let reasoning = config.reasoning_effort
        .unwrap_or(cli.reasoning);

    let task = cli.task
        .or(config.task)
        .unwrap_or_else(|| "TASK.md".to_string());

    let prompt = resolve_prompt(&task)?;

    let iterations = config.iterations
        .unwrap_or(cli.iterations);

    let runner = Runner::new(harness, model, dangerous, reasoning);

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
        let count: u32 = iterations.parse()
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
