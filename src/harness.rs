use anyhow::{bail, Result};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, error, info, instrument, trace};

use crate::process;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Harness {
    Codex,
    Claude,
    Pi,
    Gemini,
}

impl Harness {
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "codex" => Ok(Self::Codex),
            "claude" => Ok(Self::Claude),
            "pi" => Ok(Self::Pi),
            "gemini" => Ok(Self::Gemini),
            _ => bail!(
                "Unknown harness: {}. Valid options: codex, claude, pi, gemini",
                s
            ),
        }
    }

    pub fn default_model(&self) -> &'static str {
        match self {
            Self::Codex => "gpt-5.2-codex",
            Self::Claude => "claude-opus-4-5-20251101",
            Self::Pi => "claude-opus-4-5",
            Self::Gemini => "auto-gemini-3",
        }
    }

    pub fn default_provider(&self) -> &'static str {
        match self {
            Self::Pi => "anthropic",
            _ => "",
        }
    }

    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Pi => "pi",
            Self::Gemini => "gemini",
        }
    }
}

pub struct Runner {
    pub(crate) harness: Harness,
    pub(crate) model: String,
    pub(crate) dangerous: bool,
    pub(crate) reasoning_effort: String,
    pub(crate) provider: String,
}

impl Runner {
    pub fn new(
        harness: Harness,
        model: String,
        dangerous: bool,
        reasoning_effort: String,
        provider: String,
    ) -> Self {
        Self {
            harness,
            model,
            dangerous,
            reasoning_effort,
            provider,
        }
    }

    #[instrument(skip(self, prompt), fields(harness = %self.harness.command_name(), model = %self.model))]
    pub async fn run(&self, prompt: &str) -> Result<()> {
        info!(prompt_len = prompt.len(), "starting agent run");
        let result = match self.harness {
            Harness::Codex => self.run_codex(prompt).await,
            Harness::Claude => self.run_claude(prompt).await,
            Harness::Pi => self.run_pi(prompt).await,
            Harness::Gemini => self.run_gemini(prompt).await,
        };
        match &result {
            Ok(()) => info!("agent run completed successfully"),
            Err(e) => error!(error = %e, "agent run failed"),
        }
        result
    }

    async fn run_codex(&self, prompt: &str) -> Result<()> {
        let mut cmd = Command::new("codex");
        cmd.arg("exec")
            .arg("--skip-git-repo-check") // Allow running in non-git directories
            .arg("-m")
            .arg(&self.model)
            .arg("-c")
            .arg(format!(
                "model_reasoning_effort=\"{}\"",
                self.reasoning_effort
            ));

        if self.dangerous {
            cmd.arg("-c")
                .arg("approval_policy=\"never\"")
                .arg("-c")
                .arg("sandbox_mode=\"danger-full-access\"");
        }

        cmd.arg(prompt)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        debug!("spawning codex process");
        let mut child = cmd.spawn()?;
        let pid = child.id().unwrap_or(0);
        info!(pid, "codex process spawned");

        // Register the process for tracking
        let _ = process::register_process(pid, "codex", &self.model, None);

        let status = child.wait().await?;
        debug!(pid, ?status, "codex process exited");

        // Unregister when done
        let _ = process::unregister_process(pid);
        trace!(pid, "unregistered codex process");

        if !status.success() {
            error!(pid, ?status, "codex exited with non-zero status");
            bail!("codex exited with status: {}", status);
        }
        Ok(())
    }

    async fn run_claude(&self, prompt: &str) -> Result<()> {
        let mut cmd = Command::new("claude");

        if self.dangerous {
            cmd.arg("--dangerously-skip-permissions");
        }

        cmd.arg("--model")
            .arg(&self.model)
            .arg("--verbose")
            .arg("-p")
            .arg("--output-format")
            .arg("stream-json")
            .arg(prompt)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        debug!("spawning claude process");
        let mut child = cmd.spawn()?;
        let pid = child.id().unwrap_or(0);
        info!(pid, "claude process spawned");

        // Register the process for tracking
        let _ = process::register_process(pid, "claude", &self.model, None);

        let stdout = child.stdout.take().expect("Failed to capture stdout");
        let stderr = child.stderr.take().expect("Failed to capture stderr");

        let stdout_reader = BufReader::new(stdout);
        let stderr_reader = BufReader::new(stderr);

        let stdout_handle = tokio::spawn(async move {
            let mut lines = stdout_reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                    Self::process_claude_json(&json);
                }
            }
        });

        let stderr_handle = tokio::spawn(async move {
            let mut lines = stderr_reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                    Self::process_claude_json(&json);
                }
            }
        });

        let status = child.wait().await?;
        debug!(pid, ?status, "claude process exited");
        let _ = stdout_handle.await;
        let _ = stderr_handle.await;

        // Unregister when done
        let _ = process::unregister_process(pid);
        trace!(pid, "unregistered claude process");

        if !status.success() {
            error!(pid, ?status, "claude exited with non-zero status");
            bail!("claude exited with status: {}", status);
        }
        Ok(())
    }

    fn process_claude_json(json: &serde_json::Value) {
        match json.get("type").and_then(|t| t.as_str()) {
            Some("system") => {
                // Init message - optionally show session info
                if json.get("subtype").and_then(|s| s.as_str()) == Some("init") {
                    if let Some(model) = json.get("model").and_then(|m| m.as_str()) {
                        println!("claude [{}]", model);
                    }
                }
            }
            Some("assistant") => {
                // Assistant messages contain content in message.content array
                if let Some(message) = json.get("message") {
                    Self::process_claude_content(message.get("content"));
                }
            }
            Some("user") => {
                // Tool results come back as user messages
                if json.get("tool_use_result").is_some() {
                    println!("✓ done");
                }
            }
            Some("result") => {
                // Final result with stats
                if let Some(usage) = json.get("usage") {
                    let input = usage.get("input_tokens").and_then(|v| v.as_u64());
                    let output = usage.get("output_tokens").and_then(|v| v.as_u64());
                    let cache_read = usage
                        .get("cache_read_input_tokens")
                        .and_then(|v| v.as_u64());
                    if input.is_some() || output.is_some() {
                        println!(
                            "\nTokens: in:{} out:{} cached:{}",
                            input
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "-".to_string()),
                            output
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "-".to_string()),
                            cache_read
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "-".to_string())
                        );
                    }
                }
                if let Some(cost) = json.get("total_cost_usd").and_then(|c| c.as_f64()) {
                    println!("Cost: ${:.4}", cost);
                }
            }
            _ => {}
        }
    }

    fn process_claude_content(content: Option<&serde_json::Value>) {
        let Some(content) = content else { return };
        match content {
            serde_json::Value::Array(items) => {
                for item in items {
                    Self::process_claude_content_item(item);
                }
            }
            serde_json::Value::String(text) => {
                print!("{}", text);
            }
            serde_json::Value::Object(_) => {
                Self::process_claude_content_item(content);
            }
            _ => {}
        }
    }

    fn process_claude_content_item(item: &serde_json::Value) {
        match item.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                    print!("{}", text);
                }
            }
            Some("tool_use") => {
                Self::print_claude_tool_use(item);
            }
            Some("tool_result") => {
                Self::print_claude_tool_result(item);
            }
            _ => {}
        }
    }

    fn print_claude_tool_use(json: &serde_json::Value) {
        // Claude stream-json uses "name" and "input" for tool use
        let name = json
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("unknown");
        let input = json.get("input").map(|i| i.to_string()).unwrap_or_default();
        let truncated: String = input.chars().take(80).collect();
        println!("\n⚡ {} {}...", name, truncated);
    }

    fn print_claude_tool_result(_json: &serde_json::Value) {
        println!("✓ done\n");
    }

    async fn run_pi(&self, prompt: &str) -> Result<()> {
        let mut cmd = Command::new("pi");

        // Pi requires --provider and --model flags
        cmd.arg("--provider")
            .arg(&self.provider)
            .arg("--model")
            .arg(&self.model)
            .arg("-p") // Non-interactive print mode
            .arg(prompt)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        // Note: Pi has tools enabled by default (read, bash, edit, write)
        // No dangerous flag needed

        debug!("spawning pi process");
        let mut child = cmd.spawn()?;
        let pid = child.id().unwrap_or(0);
        info!(pid, provider = %self.provider, "pi process spawned");

        // Register the process for tracking
        let _ = process::register_process(pid, "pi", &self.model, None);

        let status = child.wait().await?;
        debug!(pid, ?status, "pi process exited");

        // Unregister when done
        let _ = process::unregister_process(pid);
        trace!(pid, "unregistered pi process");

        if !status.success() {
            error!(pid, ?status, "pi exited with non-zero status");
            bail!("pi exited with status: {}", status);
        }
        Ok(())
    }

    async fn run_gemini(&self, prompt: &str) -> Result<()> {
        let mut cmd = Command::new("gemini");

        if !self.model.trim().is_empty() {
            cmd.arg("-m").arg(&self.model);
        }

        // Stream JSON for structured output parsing
        cmd.arg("-o").arg("stream-json");

        // Dangerous mode uses yolo (auto-approve all tools)
        if self.dangerous {
            cmd.arg("--yolo");
        }

        // Positional prompt for non-interactive mode (one-shot)
        // Note: -p flag is deprecated
        cmd.arg(prompt)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        debug!("spawning gemini process");
        let mut child = cmd.spawn()?;
        let pid = child.id().unwrap_or(0);
        info!(pid, "gemini process spawned");

        // Register the process for tracking
        let _ = process::register_process(pid, "gemini", &self.model, None);

        let stdout = child.stdout.take().expect("Failed to capture stdout");
        let stderr = child.stderr.take().expect("Failed to capture stderr");

        let stdout_reader = BufReader::new(stdout);
        let stderr_reader = BufReader::new(stderr);

        let stdout_handle = tokio::spawn(async move {
            let mut lines = stdout_reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                    Self::process_gemini_json(&json);
                }
            }
        });

        let stderr_handle = tokio::spawn(async move {
            let mut lines = stderr_reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                    Self::process_gemini_json(&json);
                }
            }
        });

        let status = child.wait().await?;
        debug!(pid, ?status, "gemini process exited");
        let _ = stdout_handle.await;
        let _ = stderr_handle.await;

        // Unregister when done
        let _ = process::unregister_process(pid);
        trace!(pid, "unregistered gemini process");

        if !status.success() {
            error!(pid, ?status, "gemini exited with non-zero status");
            bail!("gemini exited with status: {}", status);
        }
        Ok(())
    }

    fn process_gemini_json(json: &serde_json::Value) {
        match json.get("type").and_then(|t| t.as_str()) {
            Some("init") => {
                let model = json
                    .get("model")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown");
                let session = json
                    .get("session_id")
                    .and_then(|s| s.as_str())
                    .unwrap_or("");
                let short_session: String = session.chars().take(8).collect();
                if short_session.is_empty() {
                    println!("gemini [{}]", model);
                } else {
                    println!("gemini [{}] session={}...", model, short_session);
                }
            }
            Some("message") => {
                let role = json.get("role").and_then(|r| r.as_str()).unwrap_or("");
                if role == "assistant" {
                    if let Some(content) = json.get("content") {
                        if let Some(text) = content.as_str() {
                            print!("{}", text);
                        }
                    }
                }
            }
            Some("tool_use") => {
                let name = json
                    .get("tool_name")
                    .or_else(|| json.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown");
                let params = json
                    .get("parameters")
                    .or_else(|| json.get("input"))
                    .map(|i| i.to_string())
                    .unwrap_or_default();
                let truncated: String = params.chars().take(80).collect();
                println!("\n⚡ {} {}...\n", name, truncated);
            }
            Some("tool_result") => {
                println!("✓ done\n");
            }
            Some("result") => {
                if let Some(stats) = json.get("stats") {
                    let total = stats.get("total_tokens").and_then(|v| v.as_u64());
                    let input = stats.get("input_tokens").and_then(|v| v.as_u64());
                    let output = stats.get("output_tokens").and_then(|v| v.as_u64());
                    let duration = stats.get("duration_ms").and_then(|v| v.as_u64());
                    if total.is_some() || input.is_some() || output.is_some() || duration.is_some()
                    {
                        println!(
                            "\nTokens: {} (in:{} out:{}) duration:{}ms",
                            total
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "-".to_string()),
                            input
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "-".to_string()),
                            output
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "-".to_string()),
                            duration
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "-".to_string())
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_harness_from_str_codex() {
        assert_eq!(Harness::from_str("codex").unwrap(), Harness::Codex);
        assert_eq!(Harness::from_str("CODEX").unwrap(), Harness::Codex);
        assert_eq!(Harness::from_str("Codex").unwrap(), Harness::Codex);
    }

    #[test]
    fn test_harness_from_str_claude() {
        assert_eq!(Harness::from_str("claude").unwrap(), Harness::Claude);
        assert_eq!(Harness::from_str("CLAUDE").unwrap(), Harness::Claude);
    }

    #[test]
    fn test_harness_from_str_pi() {
        assert_eq!(Harness::from_str("pi").unwrap(), Harness::Pi);
        assert_eq!(Harness::from_str("PI").unwrap(), Harness::Pi);
    }

    #[test]
    fn test_harness_from_str_gemini() {
        assert_eq!(Harness::from_str("gemini").unwrap(), Harness::Gemini);
        assert_eq!(Harness::from_str("GEMINI").unwrap(), Harness::Gemini);
    }

    #[test]
    fn test_harness_from_str_invalid() {
        let result = Harness::from_str("invalid");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Unknown harness"));
        assert!(err.contains("invalid"));
    }

    #[test]
    fn test_harness_default_model() {
        assert_eq!(Harness::Codex.default_model(), "gpt-5.2-codex");
        assert_eq!(Harness::Claude.default_model(), "claude-opus-4-5-20251101");
        assert_eq!(Harness::Pi.default_model(), "claude-opus-4-5");
        assert_eq!(Harness::Gemini.default_model(), "auto-gemini-3");
    }

    #[test]
    fn test_harness_default_provider() {
        assert_eq!(Harness::Pi.default_provider(), "anthropic");
        assert_eq!(Harness::Codex.default_provider(), "");
        assert_eq!(Harness::Claude.default_provider(), "");
        assert_eq!(Harness::Gemini.default_provider(), "");
    }

    #[test]
    fn test_harness_command_name() {
        assert_eq!(Harness::Codex.command_name(), "codex");
        assert_eq!(Harness::Claude.command_name(), "claude");
        assert_eq!(Harness::Pi.command_name(), "pi");
        assert_eq!(Harness::Gemini.command_name(), "gemini");
    }

    #[test]
    fn test_runner_new() {
        let runner = Runner::new(
            Harness::Claude,
            "test-model".to_string(),
            true,
            "high".to_string(),
            "anthropic".to_string(),
        );
        assert_eq!(runner.harness, Harness::Claude);
        assert_eq!(runner.model, "test-model");
        assert!(runner.dangerous);
        assert_eq!(runner.reasoning_effort, "high");
        assert_eq!(runner.provider, "anthropic");
    }
}
