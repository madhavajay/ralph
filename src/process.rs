use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Global process registry
static REGISTRY: LazyLock<Mutex<ProcessRegistry>> =
    LazyLock::new(|| Mutex::new(ProcessRegistry::load().unwrap_or_default()));

/// Information about a tracked process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub harness: String,
    pub model: String,
    pub working_dir: String,
    pub started_at: u64,
    pub parent_pid: u32,
    pub tmux_session: Option<String>,
}

impl ProcessInfo {
    /// Check if this process is still running
    pub fn is_alive(&self) -> bool {
        std::path::Path::new(&format!("/proc/{}", self.pid)).exists()
    }

    /// Get age in seconds
    pub fn age_secs(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.started_at)
    }

    /// Format age as human readable
    pub fn age_human(&self) -> String {
        let secs = self.age_secs();
        if secs < 60 {
            format!("{}s", secs)
        } else if secs < 3600 {
            format!("{}m", secs / 60)
        } else if secs < 86400 {
            format!("{}h", secs / 3600)
        } else {
            format!("{}d", secs / 86400)
        }
    }
}

/// Registry of tracked processes
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProcessRegistry {
    processes: HashMap<u32, ProcessInfo>,
}

impl ProcessRegistry {
    /// Get path to the pidfile
    fn pidfile_path() -> PathBuf {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(runtime_dir).join("ralph-processes.json")
    }

    /// Load registry from disk
    pub fn load() -> Result<Self> {
        let path = Self::pidfile_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read pidfile: {:?}", path))?;
        let registry: Self =
            serde_json::from_str(&content).with_context(|| "Failed to parse pidfile")?;
        Ok(registry)
    }

    /// Save registry to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::pidfile_path();
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content)
            .with_context(|| format!("Failed to write pidfile: {:?}", path))?;
        Ok(())
    }

    /// Register a new process
    pub fn register(&mut self, info: ProcessInfo) {
        self.processes.insert(info.pid, info);
    }

    /// Unregister a process
    pub fn unregister(&mut self, pid: u32) {
        self.processes.remove(&pid);
    }

    /// Get all tracked processes
    pub fn all(&self) -> Vec<&ProcessInfo> {
        self.processes.values().collect()
    }

    /// Get alive processes only
    pub fn alive(&self) -> Vec<&ProcessInfo> {
        self.processes.values().filter(|p| p.is_alive()).collect()
    }

    /// Get dead (orphaned) processes
    #[allow(dead_code)]
    pub fn dead(&self) -> Vec<&ProcessInfo> {
        self.processes.values().filter(|p| !p.is_alive()).collect()
    }

    /// Clean up dead processes from registry
    pub fn cleanup_dead(&mut self) -> usize {
        let dead_pids: Vec<u32> = self
            .processes
            .iter()
            .filter(|(_, p)| !p.is_alive())
            .map(|(pid, _)| *pid)
            .collect();
        let count = dead_pids.len();
        for pid in dead_pids {
            self.processes.remove(&pid);
        }
        count
    }

    /// Get processes for a specific working directory
    pub fn by_working_dir(&self, dir: &str) -> Vec<&ProcessInfo> {
        self.processes
            .values()
            .filter(|p| p.working_dir == dir)
            .collect()
    }

    /// Get processes for a specific harness
    pub fn by_harness(&self, harness: &str) -> Vec<&ProcessInfo> {
        self.processes
            .values()
            .filter(|p| p.harness == harness)
            .collect()
    }
}

// Public API using global registry

/// Register a spawned process
pub fn register_process(
    pid: u32,
    harness: &str,
    model: &str,
    tmux_session: Option<String>,
) -> Result<()> {
    let working_dir = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let info = ProcessInfo {
        pid,
        harness: harness.to_string(),
        model: model.to_string(),
        working_dir,
        started_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        parent_pid: std::process::id(),
        tmux_session,
    };

    let mut registry = REGISTRY.lock().unwrap();
    registry.register(info);
    registry.save()?;
    Ok(())
}

/// Unregister a process when it exits
pub fn unregister_process(pid: u32) -> Result<()> {
    let mut registry = REGISTRY.lock().unwrap();
    registry.unregister(pid);
    registry.save()?;
    Ok(())
}

/// List all tracked processes
pub fn list_processes() -> Vec<ProcessInfo> {
    let registry = REGISTRY.lock().unwrap();
    registry.all().into_iter().cloned().collect()
}

/// List alive processes
pub fn list_alive_processes() -> Vec<ProcessInfo> {
    let registry = REGISTRY.lock().unwrap();
    registry.alive().into_iter().cloned().collect()
}

/// Clean dead entries from registry
pub fn cleanup_registry() -> Result<usize> {
    let mut registry = REGISTRY.lock().unwrap();
    let count = registry.cleanup_dead();
    registry.save()?;
    Ok(count)
}

/// Kill a specific process by PID
pub fn kill_process(pid: u32) -> Result<bool> {
    use std::process::Command;

    let info = {
        let registry = REGISTRY.lock().unwrap();
        registry.processes.get(&pid).cloned()
    };

    if let Some(info) = info {
        // If it has a tmux session, kill that too
        if let Some(session) = &info.tmux_session {
            let _ = Command::new("tmux")
                .args(["kill-session", "-t", session])
                .status();
        }
    }

    // Send SIGTERM to the process
    let status = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status();

    if status.is_ok() {
        // Give it a moment to die
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Check if still alive, send SIGKILL
        if std::path::Path::new(&format!("/proc/{}", pid)).exists() {
            let _ = Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .status();
        }

        // Unregister from our tracking
        let mut registry = REGISTRY.lock().unwrap();
        registry.unregister(pid);
        let _ = registry.save();
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Kill all tracked processes
pub fn kill_all_processes() -> Result<(usize, usize)> {
    let processes = list_alive_processes();
    let total = processes.len();
    let mut killed = 0;

    for proc in processes {
        if kill_process(proc.pid).unwrap_or(false) {
            killed += 1;
        }
    }

    Ok((killed, total))
}

/// Kill processes matching a filter
pub fn kill_processes_by_dir(dir: &str) -> Result<(usize, usize)> {
    let processes = {
        let registry = REGISTRY.lock().unwrap();
        registry
            .by_working_dir(dir)
            .into_iter()
            .filter(|p| p.is_alive())
            .cloned()
            .collect::<Vec<_>>()
    };

    let total = processes.len();
    let mut killed = 0;

    for proc in processes {
        if kill_process(proc.pid).unwrap_or(false) {
            killed += 1;
        }
    }

    Ok((killed, total))
}

/// Kill processes by harness type
pub fn kill_processes_by_harness(harness: &str) -> Result<(usize, usize)> {
    let processes = {
        let registry = REGISTRY.lock().unwrap();
        registry
            .by_harness(harness)
            .into_iter()
            .filter(|p| p.is_alive())
            .cloned()
            .collect::<Vec<_>>()
    };

    let total = processes.len();
    let mut killed = 0;

    for proc in processes {
        if kill_process(proc.pid).unwrap_or(false) {
            killed += 1;
        }
    }

    Ok((killed, total))
}

/// Find and register orphaned agent processes not in our registry
/// This helps recover from crashes where we lost track
pub fn discover_orphan_processes() -> Result<Vec<ProcessInfo>> {
    use std::process::Command;

    let mut orphans = Vec::new();

    // Look for common agent processes
    for harness in &["codex", "claude", "pi", "gemini"] {
        let output = Command::new("pgrep").args(["-f", harness]).output();

        if let Ok(output) = output {
            if output.status.success() {
                let pids: Vec<u32> = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .filter_map(|s| s.trim().parse().ok())
                    .collect();

                let registry = REGISTRY.lock().unwrap();
                for pid in pids {
                    if !registry.processes.contains_key(&pid) {
                        // Get working directory
                        let cwd = fs::read_link(format!("/proc/{}/cwd", pid))
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|_| "unknown".to_string());

                        // Get start time from /proc
                        let started_at = fs::metadata(format!("/proc/{}", pid))
                            .and_then(|m| m.created())
                            .map(|t| t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs())
                            .unwrap_or(0);

                        orphans.push(ProcessInfo {
                            pid,
                            harness: harness.to_string(),
                            model: "unknown".to_string(),
                            working_dir: cwd,
                            started_at,
                            parent_pid: 0,
                            tmux_session: None,
                        });
                    }
                }
            }
        }
    }

    Ok(orphans)
}

/// Print process list in a nice format
pub fn print_processes(processes: &[ProcessInfo], show_dead: bool) {
    if processes.is_empty() {
        println!("No tracked processes.");
        return;
    }

    println!(
        "{:<8} {:<10} {:<8} {:<6} {:<40} TMUX",
        "PID", "HARNESS", "STATUS", "AGE", "WORKING_DIR"
    );
    println!("{}", "-".repeat(100));

    for proc in processes {
        let status = if proc.is_alive() { "alive" } else { "dead" };
        if !show_dead && !proc.is_alive() {
            continue;
        }
        let tmux = proc.tmux_session.as_deref().unwrap_or("-");
        let dir = if proc.working_dir.len() > 40 {
            format!("...{}", &proc.working_dir[proc.working_dir.len() - 37..])
        } else {
            proc.working_dir.clone()
        };
        println!(
            "{:<8} {:<10} {:<8} {:<6} {:<40} {}",
            proc.pid,
            proc.harness,
            status,
            proc.age_human(),
            dir,
            tmux
        );
    }
}

/// Print JSON output
pub fn print_processes_json(processes: &[ProcessInfo]) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(processes)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_info_age() {
        let info = ProcessInfo {
            pid: 12345,
            harness: "codex".to_string(),
            model: "test".to_string(),
            working_dir: "/tmp".to_string(),
            started_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - 120,
            parent_pid: 1,
            tmux_session: None,
        };

        let age = info.age_secs();
        assert!((119..=121).contains(&age));
        assert_eq!(info.age_human(), "2m");
    }

    #[test]
    fn test_registry_operations() {
        let mut registry = ProcessRegistry::default();

        let info = ProcessInfo {
            pid: 99999,
            harness: "test".to_string(),
            model: "test-model".to_string(),
            working_dir: "/tmp/test".to_string(),
            started_at: 0,
            parent_pid: 1,
            tmux_session: None,
        };

        registry.register(info.clone());
        assert_eq!(registry.all().len(), 1);

        registry.unregister(99999);
        assert_eq!(registry.all().len(), 0);
    }
}
