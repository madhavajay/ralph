use anyhow::{bail, Context, Result};
use std::time::Duration;

use crate::harness::{Harness, Runner};
use crate::tmux;

/// Parse a duration string like "1m", "5m", "10m", "1h"
pub fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim().to_lowercase();

    if s.ends_with('m') {
        let mins: u64 = s
            .trim_end_matches('m')
            .parse()
            .context("Invalid minutes value")?;
        Ok(Duration::from_secs(mins * 60))
    } else if s.ends_with('h') {
        let hours: u64 = s
            .trim_end_matches('h')
            .parse()
            .context("Invalid hours value")?;
        Ok(Duration::from_secs(hours * 3600))
    } else if s.ends_with('s') {
        let secs: u64 = s
            .trim_end_matches('s')
            .parse()
            .context("Invalid seconds value")?;
        Ok(Duration::from_secs(secs))
    } else {
        // Assume minutes if no suffix
        let mins: u64 = s.parse().context("Invalid duration value")?;
        Ok(Duration::from_secs(mins * 60))
    }
}

/// Monitor configuration
pub struct MonitorConfig {
    pub monitor_harness: Harness,
    pub monitor_model: String,
    pub monitor_interval: Duration,
    pub inner_harness: Harness,
    pub inner_model: String,
    pub task: String,
    pub dangerous: bool,
    pub reasoning_effort: String,
    pub provider: String,
}

/// Run the monitor mode
pub async fn run_monitor(config: MonitorConfig) -> Result<()> {
    // Check that tmux is available
    if !tmux::tmux_available() {
        bail!("Monitor mode requires tmux. Please install tmux first.");
    }

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let base_inner_session = format!("ralph-inner-{}", timestamp);
    let inner_session = tmux::unique_session_name(&base_inner_session);
    if inner_session != base_inner_session {
        eprintln!(
            "Session name '{}' already exists, using '{}'",
            base_inner_session, inner_session
        );
    }

    // Start the inner agent in a tmux session
    println!(
        "Starting inner agent ({}) in tmux session: {}",
        config.inner_harness.command_name(),
        inner_session
    );

    // Build the inner command
    let inner_cmd = build_ralph_command(
        &config.inner_harness,
        &config.inner_model,
        config.dangerous,
        &config.reasoning_effort,
        &config.provider,
        &config.task,
        "inf", // Inner agent runs infinitely
    );

    tmux::start_in_tmux(&inner_session, "ralph", &inner_cmd, false)?;

    println!("Inner agent started. Session: {}", inner_session);
    println!("Monitor will check every {:?}", config.monitor_interval);
    println!();

    // Create the monitor runner
    let monitor_runner = Runner::new(
        config.monitor_harness,
        config.monitor_model,
        config.dangerous,
        config.reasoning_effort,
        config.provider,
    );

    // Run the monitor loop
    loop {
        // Wait for the interval
        tokio::time::sleep(config.monitor_interval).await;

        // Check if inner session still exists
        if !tmux::session_exists(&inner_session) {
            println!("Inner session {} has ended.", inner_session);
            break;
        }

        // Capture output from inner session
        let output = tmux::capture_tmux_output(&inner_session, 100)?;

        // Create a monitoring prompt
        let monitor_prompt = create_monitor_prompt(&config.task, &output);

        println!("--- Monitor Check ---");
        println!("Checking inner agent progress...");

        // Run the monitor agent
        if let Err(e) = monitor_runner.run(&monitor_prompt).await {
            eprintln!("Monitor agent error: {}", e);
        }

        println!("--- End Monitor Check ---\n");
    }

    Ok(())
}

fn build_ralph_command(
    harness: &Harness,
    model: &str,
    dangerous: bool,
    reasoning: &str,
    provider: &str,
    task: &str,
    iterations: &str,
) -> Vec<String> {
    let mut args = vec![
        "-H".to_string(),
        harness.command_name().to_string(),
        "-m".to_string(),
        model.to_string(),
        "-n".to_string(),
        iterations.to_string(),
        "--reasoning".to_string(),
        reasoning.to_string(),
    ];

    if dangerous {
        args.push("--dangerous".to_string());
    } else {
        args.push("--safe".to_string());
    }

    if !provider.is_empty() {
        args.push("--provider".to_string());
        args.push(provider.to_string());
    }

    args.push("--no-tmux".to_string()); // Inner agent doesn't nest tmux
    args.push(task.to_string());

    args
}

fn create_monitor_prompt(original_task: &str, inner_output: &str) -> String {
    format!(
        r#"You are a monitor agent watching an inner coding agent work on a task.

ORIGINAL TASK:
{}

RECENT OUTPUT FROM INNER AGENT (last 100 lines):
{}

Your job:
1. Assess if the inner agent is making progress on the task
2. Check if it seems stuck or in an error loop
3. If needed, you can update TASK.md to provide guidance or corrections
4. Report your observations

What is the current status of the inner agent's work?"#,
        original_task, inner_output
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_minutes() {
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
        assert_eq!(parse_duration("1m").unwrap(), Duration::from_secs(60));
        assert_eq!(parse_duration("10m").unwrap(), Duration::from_secs(600));
    }

    #[test]
    fn test_parse_duration_hours() {
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_duration("2h").unwrap(), Duration::from_secs(7200));
    }

    #[test]
    fn test_parse_duration_seconds() {
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("90s").unwrap(), Duration::from_secs(90));
    }

    #[test]
    fn test_parse_duration_no_suffix() {
        // Assumes minutes
        assert_eq!(parse_duration("5").unwrap(), Duration::from_secs(300));
    }

    #[test]
    fn test_parse_duration_invalid() {
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("").is_err());
    }

    #[test]
    fn test_build_ralph_command() {
        let args = build_ralph_command(
            &Harness::Claude,
            "claude-opus",
            true,
            "medium",
            "",
            "TASK.md",
            "5",
        );
        assert!(args.contains(&"-H".to_string()));
        assert!(args.contains(&"claude".to_string()));
        assert!(args.contains(&"-m".to_string()));
        assert!(args.contains(&"claude-opus".to_string()));
        assert!(args.contains(&"--dangerous".to_string()));
        assert!(args.contains(&"--no-tmux".to_string()));
        assert!(args.contains(&"TASK.md".to_string()));
    }
}
