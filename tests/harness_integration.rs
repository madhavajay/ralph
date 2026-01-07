//! Integration tests that run against real CLI harnesses.
//!
//! These tests are ignored by default because they require:
//! - The actual CLI tools (claude, codex, pi, gemini) to be installed
//! - Valid API keys/credentials configured
//! - Network access
//!
//! Run specific harness tests with:
//!   cargo test --test harness_integration test_claude -- --ignored
//!   cargo test --test harness_integration test_codex -- --ignored
//!   cargo test --test harness_integration test_pi -- --ignored
//!   cargo test --test harness_integration test_gemini -- --ignored
//!
//! Run all harness tests:
//!   cargo test --test harness_integration -- --ignored

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

fn ralph_bin() -> String {
    env!("CARGO_BIN_EXE_ralph").to_string()
}

fn is_harness_available(harness: &str) -> bool {
    which::which(harness).is_ok()
}

#[allow(dead_code)]
fn wait_for_file(path: &Path, timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    false
}

/// Run a harness test with a simple file creation prompt
fn run_harness_file_test(harness: &str, iterations: u32) -> Result<(), String> {
    if !is_harness_available(harness) {
        return Err(format!("{} CLI not found in PATH", harness));
    }

    let temp_dir = TempDir::new().map_err(|e| e.to_string())?;
    let work_dir = temp_dir.path();

    let prompt = "Create a file called hello.txt with the contents 'Hello, World!' and nothing else. Do not create any other files.";

    let mut cmd = Command::new(ralph_bin());
    cmd.current_dir(work_dir)
        .arg("-H")
        .arg(harness)
        .arg("-n")
        .arg(iterations.to_string())
        .arg("--dangerous")
        .arg("--no-tmux") // Run in foreground for tests
        .arg(prompt);

    println!("Running: {:?}", cmd);
    println!("Work dir: {:?}", work_dir);

    let output = cmd.output().map_err(|e| e.to_string())?;

    println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("stderr: {}", String::from_utf8_lossy(&output.stderr));

    if !output.status.success() {
        return Err(format!("{} exited with status: {}", harness, output.status));
    }

    // Check that hello.txt was created
    let hello_path = work_dir.join("hello.txt");
    if !hello_path.exists() {
        // List what files were created
        let files: Vec<_> = fs::read_dir(work_dir)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
        return Err(format!("hello.txt not created. Files in dir: {:?}", files));
    }

    let contents = fs::read_to_string(&hello_path).map_err(|e| e.to_string())?;
    if !contents.contains("Hello") {
        return Err(format!("hello.txt has unexpected contents: {}", contents));
    }

    println!(
        "SUCCESS: hello.txt created with contents: {}",
        contents.trim()
    );
    Ok(())
}

// ============================================================================
// Claude Tests
// ============================================================================

#[test]
#[ignore]
fn test_claude_single_iteration() {
    if let Err(e) = run_harness_file_test("claude", 1) {
        panic!("Claude single iteration test failed: {}", e);
    }
}

#[test]
#[ignore]
fn test_claude_three_iterations() {
    if let Err(e) = run_harness_file_test("claude", 3) {
        panic!("Claude three iterations test failed: {}", e);
    }
}

// ============================================================================
// Codex Tests
// ============================================================================

#[test]
#[ignore]
fn test_codex_single_iteration() {
    if let Err(e) = run_harness_file_test("codex", 1) {
        panic!("Codex single iteration test failed: {}", e);
    }
}

#[test]
#[ignore]
fn test_codex_three_iterations() {
    if let Err(e) = run_harness_file_test("codex", 3) {
        panic!("Codex three iterations test failed: {}", e);
    }
}

// ============================================================================
// Pi Tests
// ============================================================================

#[test]
#[ignore]
fn test_pi_single_iteration() {
    if let Err(e) = run_harness_file_test("pi", 1) {
        panic!("Pi single iteration test failed: {}", e);
    }
}

#[test]
#[ignore]
fn test_pi_three_iterations() {
    if let Err(e) = run_harness_file_test("pi", 3) {
        panic!("Pi three iterations test failed: {}", e);
    }
}

// ============================================================================
// Gemini Tests
// ============================================================================

#[test]
#[ignore]
fn test_gemini_single_iteration() {
    if let Err(e) = run_harness_file_test("gemini", 1) {
        panic!("Gemini single iteration test failed: {}", e);
    }
}

#[test]
#[ignore]
fn test_gemini_three_iterations() {
    if let Err(e) = run_harness_file_test("gemini", 3) {
        panic!("Gemini three iterations test failed: {}", e);
    }
}

// ============================================================================
// Multi-file Tests (more complex)
// ============================================================================

fn run_harness_multifile_test(harness: &str) -> Result<(), String> {
    if !is_harness_available(harness) {
        return Err(format!("{} CLI not found in PATH", harness));
    }

    let temp_dir = TempDir::new().map_err(|e| e.to_string())?;
    let work_dir = temp_dir.path();

    let prompt = r#"Create exactly two files:
1. greeting.txt containing "Hello"
2. farewell.txt containing "Goodbye"
Do not create any other files."#;

    let mut cmd = Command::new(ralph_bin());
    cmd.current_dir(work_dir)
        .arg("-H")
        .arg(harness)
        .arg("-n")
        .arg("1")
        .arg("--dangerous")
        .arg("--no-tmux") // Run in foreground for tests
        .arg(prompt);

    let output = cmd.output().map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(format!(
            "{} exited with status: {}\nstderr: {}",
            harness,
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // Check files
    let greeting = work_dir.join("greeting.txt");
    let farewell = work_dir.join("farewell.txt");

    if !greeting.exists() {
        return Err("greeting.txt not created".to_string());
    }
    if !farewell.exists() {
        return Err("farewell.txt not created".to_string());
    }

    let greeting_contents = fs::read_to_string(&greeting).map_err(|e| e.to_string())?;
    let farewell_contents = fs::read_to_string(&farewell).map_err(|e| e.to_string())?;

    if !greeting_contents.contains("Hello") {
        return Err(format!(
            "greeting.txt missing 'Hello': {}",
            greeting_contents
        ));
    }
    if !farewell_contents.contains("Goodbye") {
        return Err(format!(
            "farewell.txt missing 'Goodbye': {}",
            farewell_contents
        ));
    }

    println!("SUCCESS: Both files created correctly");
    Ok(())
}

#[test]
#[ignore]
fn test_claude_multifile() {
    if let Err(e) = run_harness_multifile_test("claude") {
        panic!("Claude multifile test failed: {}", e);
    }
}

#[test]
#[ignore]
fn test_codex_multifile() {
    if let Err(e) = run_harness_multifile_test("codex") {
        panic!("Codex multifile test failed: {}", e);
    }
}

#[test]
#[ignore]
fn test_pi_multifile() {
    if let Err(e) = run_harness_multifile_test("pi") {
        panic!("Pi multifile test failed: {}", e);
    }
}

#[test]
#[ignore]
fn test_gemini_multifile() {
    if let Err(e) = run_harness_multifile_test("gemini") {
        panic!("Gemini multifile test failed: {}", e);
    }
}

// ============================================================================
// Availability check (not ignored - quick check)
// ============================================================================

#[test]
fn test_harness_availability_check() {
    println!("Harness availability:");
    println!("  claude: {}", is_harness_available("claude"));
    println!("  codex:  {}", is_harness_available("codex"));
    println!("  pi:     {}", is_harness_available("pi"));
    println!("  gemini: {}", is_harness_available("gemini"));
}
