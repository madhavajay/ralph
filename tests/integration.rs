use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn ralph_cmd() -> assert_cmd::Command {
    assert_cmd::Command::new(env!("CARGO_BIN_EXE_ralph"))
}

// ============================================================================
// Providers command tests
// ============================================================================

#[test]
fn test_providers_command() {
    ralph_cmd()
        .arg("providers")
        .assert()
        .success()
        .stdout(predicate::str::contains("Detected providers"));
}

#[test]
fn test_providers_command_json() {
    ralph_cmd()
        .args(["providers", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("["));
}

// ============================================================================
// Usage command tests
// ============================================================================

#[test]
fn test_usage_command() {
    // Usage command should work even without codexbar
    ralph_cmd().arg("usage").assert().success();
}

#[test]
fn test_usage_command_json() {
    ralph_cmd()
        .args(["usage", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("["));
}

#[test]
fn test_usage_command_specific_provider() {
    ralph_cmd()
        .args(["usage", "--provider", "claude"])
        .assert()
        .success();
}

// ============================================================================
// Monitor command tests
// ============================================================================

#[test]
fn test_monitor_command_help() {
    ralph_cmd()
        .args(["monitor", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("monitor-harness"))
        .stdout(predicate::str::contains("inner-harness"))
        .stdout(predicate::str::contains("monitor-interval"));
}

// ============================================================================
// Tmux flag tests
// ============================================================================

#[test]
fn test_help_shows_tmux_flags() {
    ralph_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--tmux"))
        .stdout(predicate::str::contains("--no-tmux"))
        .stdout(predicate::str::contains("--tmux-attach"))
        .stdout(predicate::str::contains("--tmux-session"));
}

#[test]
fn test_help_shows_usage_limit_flags() {
    ralph_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--usage-limit-daily"))
        .stdout(predicate::str::contains("--usage-limit-weekly"))
        .stdout(predicate::str::contains("--fallback-harness"));
}

#[test]
fn test_init_shows_new_config_options() {
    ralph_cmd()
        .arg("--init")
        .assert()
        .success()
        .stdout(predicate::str::contains("usage_limit_daily"))
        .stdout(predicate::str::contains("tmux"))
        .stdout(predicate::str::contains("monitor_interval"));
}

#[test]
fn test_version_flag() {
    ralph_cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("ralph"));
}

#[test]
fn test_help_flag() {
    ralph_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("CLI agent harness"))
        .stdout(predicate::str::contains("--harness"))
        .stdout(predicate::str::contains("--model"))
        .stdout(predicate::str::contains("--iterations"));
}

#[test]
fn test_list_harnesses() {
    ralph_cmd()
        .arg("--list-harnesses")
        .assert()
        .success()
        .stdout(predicate::str::contains("codex"))
        .stdout(predicate::str::contains("claude"))
        .stdout(predicate::str::contains("pi"))
        .stdout(predicate::str::contains("gemini"));
}

#[test]
fn test_init_config() {
    ralph_cmd()
        .arg("--init")
        .assert()
        .success()
        .stdout(predicate::str::contains("harness"))
        .stdout(predicate::str::contains("model"))
        .stdout(predicate::str::contains("dangerous"));
}

#[test]
fn test_invalid_harness() {
    ralph_cmd()
        .args(["-H", "invalid", "test"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unknown harness"));
}

#[test]
fn test_missing_harness_binary() {
    // Use a fake harness name that definitely won't exist
    // First we need to test with something that parses but binary is missing
    // We'll use a modified PATH to ensure binaries aren't found
    let temp_dir = TempDir::new().unwrap();

    ralph_cmd()
        .env("PATH", temp_dir.path()) // Empty PATH so no binaries found
        .args(["-H", "codex", "test task"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found in PATH"));
}

#[test]
fn test_config_file_loading() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join(".ralphrc");

    fs::write(
        &config_path,
        r#"
harness = "claude"
model = "test-model"
task = "my-task.md"
"#,
    )
    .unwrap();

    // Create the task file
    fs::write(temp_dir.path().join("my-task.md"), "test task content").unwrap();

    // Run from the temp directory with empty PATH - will fail because claude isn't in PATH
    ralph_cmd()
        .current_dir(temp_dir.path())
        .env("PATH", temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found in PATH"));
}

#[test]
fn test_task_file_not_found_uses_string() {
    // When a file doesn't exist, it should use the string as the prompt
    // Use empty PATH to ensure we get the "not found" error
    let temp_dir = TempDir::new().unwrap();

    ralph_cmd()
        .env("PATH", temp_dir.path())
        .args(["-H", "claude", "this is a prompt not a file"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found in PATH"));
}

#[test]
fn test_invalid_iteration_count() {
    // Create a fake "codex" binary that just exits 0
    let temp_dir = TempDir::new().unwrap();
    let fake_bin = temp_dir.path().join("codex");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::write(&fake_bin, "#!/bin/sh\nexit 0").unwrap();
        fs::set_permissions(&fake_bin, fs::Permissions::from_mode(0o755)).unwrap();
    }
    #[cfg(windows)]
    {
        fs::write(temp_dir.path().join("codex.cmd"), "@echo off\nexit /b 0").unwrap();
    }

    ralph_cmd()
        .env("PATH", temp_dir.path())
        .args(["-H", "codex", "-n", "abc", "test"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid iteration count"));
}

#[test]
fn test_zero_iterations() {
    // Create a fake "codex" binary that just exits 0
    let temp_dir = TempDir::new().unwrap();
    let fake_bin = temp_dir.path().join("codex");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::write(&fake_bin, "#!/bin/sh\nexit 0").unwrap();
        fs::set_permissions(&fake_bin, fs::Permissions::from_mode(0o755)).unwrap();
    }
    #[cfg(windows)]
    {
        fs::write(temp_dir.path().join("codex.cmd"), "@echo off\nexit /b 0").unwrap();
    }

    ralph_cmd()
        .env("PATH", temp_dir.path())
        .args(["-H", "codex", "-n", "0", "test"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must be at least 1"));
}
