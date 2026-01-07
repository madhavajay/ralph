use anyhow::{bail, Context, Result};
use std::process::Command;

/// Check if tmux is available
pub fn tmux_available() -> bool {
    which::which("tmux").is_ok()
}

/// Generate a tmux session name
pub fn generate_session_name(prefix: &str, harness: &str) -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{}-{}-{}", prefix, harness, timestamp)
}

/// Start a command in a new tmux session
pub fn start_in_tmux(
    session_name: &str,
    command: &str,
    args: &[String],
    attach: bool,
) -> Result<()> {
    // Build the full command string
    let full_command = format!("{} {}", command, args.join(" "));

    // Create new detached tmux session
    let status = Command::new("tmux")
        .args(["new-session", "-d", "-s", session_name, &full_command])
        .status()
        .context("Failed to start tmux session")?;

    if !status.success() {
        bail!("Failed to create tmux session: {}", session_name);
    }

    println!("Started tmux session: {}", session_name);
    println!("Attach with: tmux attach -t {}", session_name);

    if attach {
        // Attach to the session
        let status = Command::new("tmux")
            .args(["attach", "-t", session_name])
            .status()
            .context("Failed to attach to tmux session")?;

        if !status.success() {
            bail!("Failed to attach to tmux session: {}", session_name);
        }
    }

    Ok(())
}

/// Get the last N lines of output from a tmux session
pub fn capture_tmux_output(session_name: &str, lines: u32) -> Result<String> {
    let output = Command::new("tmux")
        .args([
            "capture-pane",
            "-t",
            session_name,
            "-p",
            "-S",
            &format!("-{}", lines),
        ])
        .output()
        .context("Failed to capture tmux pane")?;

    if !output.status.success() {
        bail!(
            "Failed to capture tmux output: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Send keys to a tmux session
#[allow(dead_code)]
pub fn send_keys(session_name: &str, keys: &str) -> Result<()> {
    let status = Command::new("tmux")
        .args(["send-keys", "-t", session_name, keys, "Enter"])
        .status()
        .context("Failed to send keys to tmux")?;

    if !status.success() {
        bail!("Failed to send keys to tmux session: {}", session_name);
    }

    Ok(())
}

/// Check if a tmux session exists
pub fn session_exists(session_name: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", session_name])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Kill a tmux session
#[allow(dead_code)]
pub fn kill_session(session_name: &str) -> Result<()> {
    let status = Command::new("tmux")
        .args(["kill-session", "-t", session_name])
        .status()
        .context("Failed to kill tmux session")?;

    if !status.success() {
        bail!("Failed to kill tmux session: {}", session_name);
    }

    Ok(())
}

/// List all ralph tmux sessions
#[allow(dead_code)]
pub fn list_ralph_sessions() -> Result<Vec<String>> {
    let output = Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output()
        .context("Failed to list tmux sessions")?;

    if !output.status.success() {
        // No sessions is not an error
        return Ok(vec![]);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let sessions: Vec<String> = stdout
        .lines()
        .filter(|line| line.starts_with("ralph"))
        .map(|s| s.to_string())
        .collect();

    Ok(sessions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_session_name() {
        let name = generate_session_name("ralph", "codex");
        assert!(name.starts_with("ralph-codex-"));
        assert!(name.len() > "ralph-codex-".len());
    }

    #[test]
    fn test_tmux_available() {
        // This test just ensures the function doesn't panic
        let _ = tmux_available();
    }
}
