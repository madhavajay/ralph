use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Global process registry
static REGISTRY: LazyLock<Mutex<ProcessRegistry>> =
    LazyLock::new(|| Mutex::new(ProcessRegistry::load().unwrap_or_default()));

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn process_is_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        if std::path::Path::new(&format!("/proc/{}", pid)).exists() {
            return true;
        }
        unix_pid_exists(pid)
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    {
        unix_pid_exists(pid)
    }

    #[cfg(windows)]
    {
        windows_pid_exists(pid)
    }
}

fn send_terminate(pid: u32) -> bool {
    #[cfg(unix)]
    {
        unix_send_signal(pid, "-TERM")
    }

    #[cfg(windows)]
    {
        windows_taskkill(pid, false)
    }
}

fn send_kill(pid: u32) -> bool {
    #[cfg(unix)]
    {
        unix_send_signal(pid, "-KILL")
    }

    #[cfg(windows)]
    {
        windows_taskkill(pid, true)
    }
}

#[cfg(unix)]
fn unix_send_signal(pid: u32, signal: &str) -> bool {
    std::process::Command::new("kill")
        .args([signal, &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn windows_taskkill(pid: u32, force: bool) -> bool {
    let mut args = vec!["/PID".to_string(), pid.to_string(), "/T".to_string()];
    if force {
        args.push("/F".to_string());
    }
    std::process::Command::new("taskkill")
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(unix)]
fn unix_pid_exists(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn windows_pid_exists(pid: u32) -> bool {
    let output = std::process::Command::new("tasklist")
        .args(["/FO", "CSV", "/NH", "/FI", &format!("PID eq {}", pid)])
        .output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .any(|line| line.contains(&format!("\"{}\"", pid)))
}

#[cfg(target_os = "linux")]
fn process_cwd(pid: u32) -> Option<String> {
    fs::read_link(format!("/proc/{}/cwd", pid))
        .map(|p| p.display().to_string())
        .ok()
}

#[cfg(not(target_os = "linux"))]
fn process_cwd(_pid: u32) -> Option<String> {
    None
}

#[cfg(target_os = "linux")]
fn process_started_at(pid: u32) -> Option<u64> {
    fs::metadata(format!("/proc/{}", pid))
        .and_then(|m| m.created())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

#[cfg(not(target_os = "linux"))]
fn process_started_at(_pid: u32) -> Option<u64> {
    None
}

#[cfg(unix)]
fn find_pids_by_name(name: &str) -> Vec<u32> {
    let output = std::process::Command::new("pgrep")
        .args(["-f", name])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|s| s.trim().parse().ok())
        .collect()
}

#[cfg(windows)]
fn find_pids_by_name(name: &str) -> Vec<u32> {
    let output = std::process::Command::new("tasklist")
        .args(["/FO", "CSV", "/NH"])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let needle = name.to_lowercase();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with("INFO:") {
                return None;
            }
            let line = line.trim_matches('"');
            let mut parts = line.split("\",\"");
            let image = parts.next()?.to_lowercase();
            let pid_str = parts.next()?;
            if image.contains(&needle) {
                pid_str.parse::<u32>().ok()
            } else {
                None
            }
        })
        .collect()
}

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
        process_is_alive(self.pid)
    }

    /// Get age in seconds
    pub fn age_secs(&self) -> u64 {
        now_secs().saturating_sub(self.started_at)
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
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir());
        runtime_dir.join("ralph-processes.json")
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
        let _lock = acquire_lock(&path)?;
        let content = serde_json::to_string_pretty(self)?;
        write_atomic(&path, &content)
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

struct LockGuard {
    path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn acquire_lock(path: &Path) -> Result<LockGuard> {
    let lock_path = path.with_extension("lock");
    let pid = std::process::id();
    let start = Instant::now();
    let timeout = Duration::from_secs(2);

    loop {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                let _ = writeln!(file, "{}", pid);
                return Ok(LockGuard { path: lock_path });
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                if let Ok(contents) = fs::read_to_string(&lock_path) {
                    match contents.trim().parse::<u32>() {
                        Ok(lock_pid) => {
                            if !process_is_alive(lock_pid) {
                                let _ = fs::remove_file(&lock_path);
                                continue;
                            }
                        }
                        Err(_) => {
                            let _ = fs::remove_file(&lock_path);
                            continue;
                        }
                    }
                }
                if start.elapsed() >= timeout {
                    return Err(anyhow!(
                        "Timed out waiting for process registry lock: {}",
                        lock_path.display()
                    ));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(err) => return Err(err.into()),
        }
    }
}

fn write_atomic(path: &Path, content: &str) -> Result<()> {
    let tmp_path = path.with_extension(format!("json.tmp.{}", std::process::id()));
    fs::write(&tmp_path, content)?;
    if let Err(err) = fs::rename(&tmp_path, path) {
        if path.exists() {
            let _ = fs::remove_file(path);
            fs::rename(&tmp_path, path)?;
        } else {
            let _ = fs::remove_file(&tmp_path);
            return Err(err.into());
        }
    }
    Ok(())
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
        started_at: now_secs(),
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
    let info = {
        let registry = REGISTRY.lock().unwrap();
        registry.processes.get(&pid).cloned()
    };

    if let Some(info) = info {
        // If it has a tmux session, kill that too
        if let Some(session) = &info.tmux_session {
            let _ = std::process::Command::new("tmux")
                .args(["kill-session", "-t", session])
                .status();
        }
    }

    let alive_before = process_is_alive(pid);
    let terminated = if alive_before {
        send_terminate(pid)
    } else {
        true
    };

    if terminated {
        // Give it a moment to die
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Check if still alive, send hard kill
        if process_is_alive(pid) {
            let _ = send_kill(pid);
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
    let mut orphans = Vec::new();

    // Look for common agent processes
    for harness in &["codex", "claude", "pi", "gemini"] {
        let pids = find_pids_by_name(harness);
        let registry = REGISTRY.lock().unwrap();
        for pid in pids {
            if !registry.processes.contains_key(&pid) {
                let cwd = process_cwd(pid).unwrap_or_else(|| "unknown".to_string());
                let started_at = process_started_at(pid).unwrap_or_else(now_secs);
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
