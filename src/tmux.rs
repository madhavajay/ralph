use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::process::Command;

/// Check if tmux is available
pub fn tmux_available() -> bool {
    which::which("tmux").is_ok()
}

/// Generate a tmux session name
pub fn generate_session_name(prefix: &str, harness: &str) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let pid = std::process::id();
    format!(
        "{}-{}-{}-{}-{}",
        prefix,
        harness,
        now.as_secs(),
        now.subsec_nanos(),
        pid
    )
}

/// Ensure a tmux session name is unique by suffixing when needed.
pub fn unique_session_name(base: &str) -> String {
    if !session_exists(base) {
        return base.to_string();
    }

    let mut counter = 1;
    loop {
        let candidate = format!("{}-{}", base, counter);
        if !session_exists(&candidate) {
            return candidate;
        }
        counter += 1;
    }
}

/// Start a command in a new tmux session
pub fn start_in_tmux(
    session_name: &str,
    command: &str,
    args: &[String],
    attach: bool,
) -> Result<()> {
    // Create new detached tmux session
    let status = Command::new("tmux")
        .args(["new-session", "-d", "-s", session_name])
        .arg(command)
        .args(args)
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

#[derive(Debug, Clone, Serialize)]
pub struct TmuxSession {
    pub name: String,
    pub attached: bool,
    pub windows: u32,
    pub created: String,
}

/// Attach to a tmux session
pub fn attach_session(session_name: &str) -> Result<()> {
    let status = Command::new("tmux")
        .args(["attach", "-t", session_name])
        .status()
        .context("Failed to attach to tmux session")?;

    if !status.success() {
        bail!("Failed to attach to tmux session: {}", session_name);
    }

    Ok(())
}

/// List all ralph tmux sessions with metadata
pub fn list_ralph_sessions() -> Result<Vec<TmuxSession>> {
    let output = Command::new("tmux")
        .args([
            "list-sessions",
            "-F",
            "#{session_name}\t#{session_attached}\t#{session_windows}\t#{session_created_string}",
        ])
        .output()
        .context("Failed to list tmux sessions")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("no server running") {
            return Ok(vec![]);
        }
        bail!("Failed to list tmux sessions: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut sessions = Vec::new();
    for line in stdout.lines() {
        let mut parts = line.split('\t');
        let name = match parts.next() {
            Some(value) => value,
            None => continue,
        };
        if !name.starts_with("ralph") {
            continue;
        }
        let attached = parts.next().unwrap_or("0") == "1";
        let windows = parts
            .next()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0);
        let created = parts.next().unwrap_or("").to_string();

        sessions.push(TmuxSession {
            name: name.to_string(),
            attached,
            windows,
            created,
        });
    }

    Ok(sessions)
}

pub fn session_label(session: &TmuxSession) -> String {
    let status = if session.attached {
        "attached"
    } else {
        "detached"
    };
    format!(
        "{}  [{}]  {}w  {}",
        session.name, status, session.windows, session.created
    )
}

pub fn print_sessions(sessions: &[TmuxSession]) {
    println!("{:<24} {:<9} {:<7} CREATED", "SESSION", "STATUS", "WINDOWS");
    for session in sessions {
        let status = if session.attached {
            "attached"
        } else {
            "detached"
        };
        println!(
            "{:<24} {:<9} {:<7} {}",
            session.name, status, session.windows, session.created
        );
    }
}

pub fn print_sessions_json(sessions: &[TmuxSession]) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(sessions)?);
    Ok(())
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
