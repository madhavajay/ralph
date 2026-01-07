use anyhow::Result;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Represents a detected provider with its metadata
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub name: String,
    pub command: &'static str,
    pub version: Option<String>,
    pub detected: bool,
    pub cli_name: &'static str,
}

/// All supported providers
pub const PROVIDERS: &[(&str, &str, &str)] = &[
    // (name, command, cli_name)
    ("codex", "codex", "codex-cli"),
    ("claude", "claude", "claude-code"),
    ("gemini", "gemini", "gemini-cli"),
    ("pi", "pi", "pi-cli"),
    ("cursor", "cursor", "cursor"),
    ("copilot", "copilot", "copilot"),
    ("augment", "augment", "augment"),
    ("kiro", "kiro", "kiro"),
    ("antigravity", "antigravity", "antigravity"),
];

impl ProviderInfo {
    pub fn new(name: &str, command: &'static str, cli_name: &'static str) -> Self {
        Self {
            name: name.to_string(),
            command,
            version: None,
            detected: false,
            cli_name,
        }
    }
}

/// Check if codexbar is available
pub fn codexbar_available() -> bool {
    which::which("codexbar").is_ok()
}

/// Detect a provider using codexbar if available, otherwise fall back to which + version
pub fn detect_provider(name: &str, command: &'static str, cli_name: &'static str) -> ProviderInfo {
    let mut info = ProviderInfo::new(name, command, cli_name);

    // Try codexbar first if available
    if codexbar_available() {
        if let Some(version) = detect_via_codexbar(name) {
            info.detected = true;
            info.version = Some(version);
            return info;
        }
    }

    // Fall back to which + version detection
    if let Ok(path) = which::which(command) {
        info.detected = true;
        info.version = detect_version(command, &path);
    }

    info
}

/// Detect provider via codexbar
fn detect_via_codexbar(provider: &str) -> Option<String> {
    let mut cmd = Command::new("codexbar");
    cmd.args(["--provider", provider, "--source", "cli"]);
    let output = command_output_with_timeout(cmd, Duration::from_secs(5))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Parse version from codexbar output
        // Format varies, but typically includes version info
        for line in stdout.lines() {
            if line.contains("version") || line.contains("Version") {
                if let Some(v) = extract_version(line) {
                    return Some(v);
                }
            }
        }
        // If we got success but no version, return a placeholder
        Some("detected".to_string())
    } else {
        None
    }
}

/// Detect version by running command --version
fn detect_version(command: &str, _path: &std::path::Path) -> Option<String> {
    let output = Command::new(command).arg("--version").output().ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        extract_version(&stdout)
    } else {
        // Some CLIs output version to stderr
        let stderr = String::from_utf8_lossy(&output.stderr);
        extract_version(&stderr)
    }
}

fn command_output_with_timeout(
    mut cmd: Command,
    timeout: Duration,
) -> Option<std::process::Output> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().ok()?;
    let start = Instant::now();

    loop {
        if let Ok(Some(_)) = child.try_wait() {
            return child.wait_with_output().ok();
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Extract version number from a string
fn extract_version(s: &str) -> Option<String> {
    // Look for version patterns like "1.0.0", "v1.0.0", "version 1.0.0"
    let s = s.trim();

    // Try to find a semver-like pattern
    for word in s.split_whitespace() {
        let word = word.trim_start_matches('v');
        if word.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            // Looks like it starts with a digit
            let version: String = word
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if !version.is_empty() && version.contains('.') {
                return Some(version);
            }
        }
    }

    // If no semver found, just return the first line trimmed
    s.lines().next().map(|l| l.trim().to_string())
}

/// Detect all providers
pub fn detect_all_providers() -> Vec<ProviderInfo> {
    PROVIDERS
        .iter()
        .map(|(name, cmd, cli_name)| detect_provider(name, cmd, cli_name))
        .collect()
}

/// Print providers in human-readable format
pub fn print_providers(providers: &[ProviderInfo]) {
    println!("Detected providers:");

    let mut detected = Vec::new();
    let mut not_detected = Vec::new();

    for p in providers {
        if p.detected {
            detected.push(p);
        } else {
            not_detected.push(p);
        }
    }

    for p in &detected {
        let version = p.version.as_deref().unwrap_or("-");
        println!("  \u{2713} {:<12} {:<12} ({})", p.name, version, p.cli_name);
    }

    for p in &not_detected {
        println!("  \u{2717} {:<12} {:<12} (not detected)", p.name, "-");
    }

    if !not_detected.is_empty() {
        println!();
        println!("Supported but not detected:");
        for p in &not_detected {
            println!("  - {} ({})", p.name, p.cli_name);
        }
    }
}

/// Print providers as JSON
pub fn print_providers_json(providers: &[ProviderInfo]) -> Result<()> {
    let json_providers: Vec<serde_json::Value> = providers
        .iter()
        .map(|p| {
            serde_json::json!({
                "name": p.name,
                "command": p.command,
                "version": p.version,
                "detected": p.detected,
                "cli_name": p.cli_name,
            })
        })
        .collect();

    println!("{}", serde_json::to_string_pretty(&json_providers)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_version_semver() {
        assert_eq!(extract_version("1.0.0"), Some("1.0.0".to_string()));
        assert_eq!(extract_version("v1.2.3"), Some("1.2.3".to_string()));
        assert_eq!(
            extract_version("claude version 1.0.5"),
            Some("1.0.5".to_string())
        );
        assert_eq!(
            extract_version("codex-cli 0.77.0"),
            Some("0.77.0".to_string())
        );
    }

    #[test]
    fn test_extract_version_fallback() {
        assert_eq!(extract_version("some text"), Some("some text".to_string()));
    }

    #[test]
    fn test_provider_info_new() {
        let info = ProviderInfo::new("codex", "codex", "codex-cli");
        assert_eq!(info.name, "codex");
        assert_eq!(info.command, "codex");
        assert_eq!(info.cli_name, "codex-cli");
        assert!(!info.detected);
        assert!(info.version.is_none());
    }

    #[test]
    fn test_providers_list() {
        assert!(PROVIDERS.len() >= 4);
        assert!(PROVIDERS.iter().any(|(name, _, _)| *name == "codex"));
        assert!(PROVIDERS.iter().any(|(name, _, _)| *name == "claude"));
        assert!(PROVIDERS.iter().any(|(name, _, _)| *name == "gemini"));
        assert!(PROVIDERS.iter().any(|(name, _, _)| *name == "pi"));
    }
}
