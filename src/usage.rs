use anyhow::Result;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::provider::{codexbar_available, detect_all_providers, ProviderInfo};

/// Usage information for a provider
#[derive(Debug, Clone)]
pub struct UsageInfo {
    pub provider: String,
    pub version: Option<String>,
    pub cli_name: String,
    pub session_percent: Option<u8>,
    pub session_reset: Option<String>,
    pub weekly_percent: Option<u8>,
    pub weekly_reset: Option<String>,
    pub account: Option<String>,
    pub plan: Option<String>,
    pub error: Option<String>,
}

impl UsageInfo {
    pub fn new(provider: &ProviderInfo) -> Self {
        Self {
            provider: provider.name.clone(),
            version: provider.version.clone(),
            cli_name: provider.cli_name.to_string(),
            session_percent: None,
            session_reset: None,
            weekly_percent: None,
            weekly_reset: None,
            account: None,
            plan: None,
            error: None,
        }
    }

    pub fn with_error(provider: &ProviderInfo, error: String) -> Self {
        let mut info = Self::new(provider);
        info.error = Some(error);
        info
    }
}

/// Get usage information for a specific provider via codexbar
fn get_usage_via_codexbar(provider: &str) -> Option<UsageInfo> {
    let mut cmd = Command::new("codexbar");
    cmd.args(["usage", "--provider", provider, "--source", "cli"]);
    let output = command_output_with_timeout(cmd, Duration::from_secs(5))?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_codexbar_usage(&stdout, provider)
}

/// Parse codexbar usage output
fn parse_codexbar_usage(output: &str, provider: &str) -> Option<UsageInfo> {
    let mut info = UsageInfo {
        provider: provider.to_string(),
        version: None,
        cli_name: String::new(),
        session_percent: None,
        session_reset: None,
        weekly_percent: None,
        weekly_reset: None,
        account: None,
        plan: None,
        error: None,
    };

    for line in output.lines() {
        let line = line.trim();
        if line.contains("Session:") {
            // Parse "Session: 95% left (resets 12:01 PM)"
            if let Some(pct) = extract_percentage(line) {
                info.session_percent = Some(pct);
            }
            if let Some(reset) = extract_reset_time(line) {
                info.session_reset = Some(reset);
            }
        } else if line.contains("Weekly:") {
            // Parse "Weekly: 52% left (resets 10 Jan 2026)"
            if let Some(pct) = extract_percentage(line) {
                info.weekly_percent = Some(pct);
            }
            if let Some(reset) = extract_reset_time(line) {
                info.weekly_reset = Some(reset);
            }
        } else if line.contains("Account:") {
            info.account = line.split(':').nth(1).map(|s| s.trim().to_string());
        } else if line.contains("Plan:") {
            info.plan = line.split(':').nth(1).map(|s| s.trim().to_string());
        }
    }

    Some(info)
}

fn extract_percentage(s: &str) -> Option<u8> {
    // Look for "XX% left" pattern
    for word in s.split_whitespace() {
        if word.ends_with('%') {
            if let Ok(n) = word.trim_end_matches('%').parse::<u8>() {
                return Some(n);
            }
        }
    }
    None
}

fn extract_reset_time(s: &str) -> Option<String> {
    // Look for "(resets ...)" pattern
    if let Some(start) = s.find("(resets ") {
        if let Some(end) = s[start..].find(')') {
            return Some(s[start + 8..start + end].to_string());
        }
    }
    None
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

/// Get usage for all detected providers
pub fn get_all_usage() -> Vec<UsageInfo> {
    let providers = detect_all_providers();
    let detected: Vec<_> = providers.iter().filter(|p| p.detected).collect();

    if detected.is_empty() {
        return vec![];
    }

    if codexbar_available() {
        // Try to get usage via codexbar for all providers
        detected
            .iter()
            .map(|p| {
                get_usage_via_codexbar(&p.name).unwrap_or_else(|| {
                    UsageInfo::with_error(p, "Unable to fetch usage".to_string())
                })
            })
            .collect()
    } else {
        // Without codexbar, we can't get usage info
        detected
            .iter()
            .map(|p| UsageInfo::with_error(p, "codexbar not installed".to_string()))
            .collect()
    }
}

/// Get usage for a specific provider
pub fn get_provider_usage(provider_name: &str) -> Option<UsageInfo> {
    let providers = detect_all_providers();
    let provider = providers.iter().find(|p| p.name == provider_name)?;

    if !provider.detected {
        return Some(UsageInfo::with_error(
            provider,
            format!("{} not detected", provider_name),
        ));
    }

    if codexbar_available() {
        get_usage_via_codexbar(provider_name).or_else(|| {
            Some(UsageInfo::with_error(
                provider,
                "Unable to fetch usage".to_string(),
            ))
        })
    } else {
        Some(UsageInfo::with_error(
            provider,
            "codexbar not installed".to_string(),
        ))
    }
}

/// Print usage in human-readable format
pub fn print_usage(usage_list: &[UsageInfo]) {
    if usage_list.is_empty() {
        println!("No providers detected. Run 'ralph providers' to see available providers.");
        return;
    }

    for usage in usage_list {
        let version = usage.version.as_deref().unwrap_or("-");
        println!("{} {} ({})", usage.provider, version, usage.cli_name);

        if let Some(err) = &usage.error {
            println!("  Error: {}", err);
        } else {
            if let Some(session) = usage.session_percent {
                let reset = usage.session_reset.as_deref().unwrap_or("unknown");
                println!("  Session: {}% left (resets {})", session, reset);
            }
            if let Some(weekly) = usage.weekly_percent {
                let reset = usage.weekly_reset.as_deref().unwrap_or("unknown");
                println!("  Weekly: {}% left (resets {})", weekly, reset);
            }
            if let Some(account) = &usage.account {
                println!("  Account: {}", account);
            }
            if let Some(plan) = &usage.plan {
                println!("  Plan: {}", plan);
            }
        }
        println!();
    }
}

/// Print usage as JSON
pub fn print_usage_json(usage_list: &[UsageInfo]) -> Result<()> {
    let json_usage: Vec<serde_json::Value> = usage_list
        .iter()
        .map(|u| {
            serde_json::json!({
                "provider": u.provider,
                "version": u.version,
                "cli_name": u.cli_name,
                "session_percent": u.session_percent,
                "session_reset": u.session_reset,
                "weekly_percent": u.weekly_percent,
                "weekly_reset": u.weekly_reset,
                "account": u.account,
                "plan": u.plan,
                "error": u.error,
            })
        })
        .collect();

    println!("{}", serde_json::to_string_pretty(&json_usage)?);
    Ok(())
}

#[derive(Debug, Default)]
pub struct UsageLimitCheck {
    pub exceeded: bool,
    pub reasons: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn check_usage_limits(
    provider: &str,
    daily_limit: Option<u8>,
    weekly_limit: Option<u8>,
) -> UsageLimitCheck {
    let mut check = UsageLimitCheck::default();

    if daily_limit.is_none() && weekly_limit.is_none() {
        return check;
    }

    let info = match get_provider_usage(provider) {
        Some(info) => info,
        None => {
            check
                .warnings
                .push(format!("Unknown provider for usage check: {}", provider));
            return check;
        }
    };

    check_usage_limits_with_info(&info, daily_limit, weekly_limit)
}

fn check_usage_limits_with_info(
    usage: &UsageInfo,
    daily_limit: Option<u8>,
    weekly_limit: Option<u8>,
) -> UsageLimitCheck {
    let mut check = UsageLimitCheck::default();

    if let Some(err) = &usage.error {
        check
            .warnings
            .push(format!("Usage unavailable for {}: {}", usage.provider, err));
        return check;
    }

    if let Some(limit) = daily_limit {
        match usage.session_percent {
            Some(left) => {
                let used = used_percent(left);
                if used >= limit {
                    check
                        .reasons
                        .push(format!("daily usage {}% used (limit {}%)", used, limit));
                }
            }
            None => {
                check.warnings.push(format!(
                    "Daily usage data unavailable for {}",
                    usage.provider
                ));
            }
        }
    }

    if let Some(limit) = weekly_limit {
        match usage.weekly_percent {
            Some(left) => {
                let used = used_percent(left);
                if used >= limit {
                    check
                        .reasons
                        .push(format!("weekly usage {}% used (limit {}%)", used, limit));
                }
            }
            None => {
                check.warnings.push(format!(
                    "Weekly usage data unavailable for {}",
                    usage.provider
                ));
            }
        }
    }

    check.exceeded = !check.reasons.is_empty();
    check
}

fn used_percent(left: u8) -> u8 {
    100u8.saturating_sub(left)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_percentage() {
        assert_eq!(extract_percentage("Session: 95% left"), Some(95));
        assert_eq!(extract_percentage("Weekly: 52% left"), Some(52));
        assert_eq!(extract_percentage("no percent here"), None);
    }

    #[test]
    fn test_extract_reset_time() {
        assert_eq!(
            extract_reset_time("Session: 95% left (resets 12:01 PM)"),
            Some("12:01 PM".to_string())
        );
        assert_eq!(
            extract_reset_time("Weekly: 52% left (resets 10 Jan 2026)"),
            Some("10 Jan 2026".to_string())
        );
        assert_eq!(extract_reset_time("no reset here"), None);
    }

    #[test]
    fn test_parse_codexbar_usage() {
        let output = r#"
Session: 95% left (resets 12:01 PM)
Weekly: 52% left (resets 10 Jan 2026)
Account: user@example.com
Plan: Pro
"#;
        let info = parse_codexbar_usage(output, "codex").unwrap();
        assert_eq!(info.provider, "codex");
        assert_eq!(info.session_percent, Some(95));
        assert_eq!(info.session_reset, Some("12:01 PM".to_string()));
        assert_eq!(info.weekly_percent, Some(52));
        assert_eq!(info.weekly_reset, Some("10 Jan 2026".to_string()));
        assert_eq!(info.account, Some("user@example.com".to_string()));
        assert_eq!(info.plan, Some("Pro".to_string()));
    }

    #[test]
    fn test_usage_info_with_error() {
        let provider = crate::provider::ProviderInfo::new("test", "test", "test-cli");
        let info = UsageInfo::with_error(&provider, "test error".to_string());
        assert_eq!(info.provider, "test");
        assert_eq!(info.error, Some("test error".to_string()));
    }

    #[test]
    fn test_usage_limit_check_exceeded() {
        let usage = UsageInfo {
            provider: "codex".to_string(),
            version: None,
            cli_name: "codex-cli".to_string(),
            session_percent: Some(10),
            session_reset: None,
            weekly_percent: Some(40),
            weekly_reset: None,
            account: None,
            plan: None,
            error: None,
        };

        let check = check_usage_limits_with_info(&usage, Some(80), Some(50));
        assert!(check.exceeded);
        assert_eq!(check.reasons.len(), 2);
        assert!(check.warnings.is_empty());
    }

    #[test]
    fn test_usage_limit_check_missing_data_warns() {
        let usage = UsageInfo {
            provider: "claude".to_string(),
            version: None,
            cli_name: "claude-code".to_string(),
            session_percent: None,
            session_reset: None,
            weekly_percent: Some(90),
            weekly_reset: None,
            account: None,
            plan: None,
            error: None,
        };

        let check = check_usage_limits_with_info(&usage, Some(80), Some(95));
        assert!(!check.exceeded);
        assert_eq!(check.warnings.len(), 1);
    }
}
