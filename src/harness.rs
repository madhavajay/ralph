use anyhow::{bail, Result};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

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
            Self::Gemini => "gemini-2.5-pro",
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

    pub async fn run(&self, prompt: &str) -> Result<()> {
        match self.harness {
            Harness::Codex => self.run_codex(prompt).await,
            Harness::Claude => self.run_claude(prompt).await,
            Harness::Pi => self.run_pi(prompt).await,
            Harness::Gemini => self.run_gemini(prompt).await,
        }
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

        let status = cmd.status().await?;
        if !status.success() {
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

        let mut child = cmd.spawn()?;

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
        let _ = stdout_handle.await;
        let _ = stderr_handle.await;

        if !status.success() {
            bail!("claude exited with status: {}", status);
        }
        Ok(())
    }

    fn process_claude_json(json: &serde_json::Value) {
        match json.get("type").and_then(|t| t.as_str()) {
            Some("assistant") => {
                if let Some(content) = json.get("message").and_then(|m| m.get("content")) {
                    print!("{}", content);
                }
            }
            Some("tool_use") => {
                let name = json
                    .get("tool_name")
                    .or_else(|| json.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown");
                let input = json
                    .get("tool_input")
                    .or_else(|| json.get("input"))
                    .map(|i| i.to_string())
                    .unwrap_or_default();
                let truncated: String = input.chars().take(80).collect();
                println!("\n⚡ {} {}...\n", name, truncated);
            }
            Some("tool_result") => {
                println!("✓ done\n");
            }
            Some("result") => {
                if let Some(result) = json.get("result") {
                    println!("{}", result);
                }
            }
            _ => {}
        }
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

        let status = cmd.status().await?;
        if !status.success() {
            bail!("pi exited with status: {}", status);
        }
        Ok(())
    }

    async fn run_gemini(&self, prompt: &str) -> Result<()> {
        let mut cmd = Command::new("gemini");

        // Model selection
        cmd.arg("--model").arg(&self.model);

        // Dangerous mode uses yolo (auto-approve all tools)
        if self.dangerous {
            cmd.arg("--yolo");
        }

        // Positional prompt for non-interactive mode (one-shot)
        // Note: -p flag is deprecated
        cmd.arg(prompt);

        cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());

        let status = cmd.status().await?;
        if !status.success() {
            bail!("gemini exited with status: {}", status);
        }
        Ok(())
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
        assert_eq!(Harness::Gemini.default_model(), "gemini-2.5-pro");
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
