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
            _ => bail!("Unknown harness: {}. Valid options: codex, claude, pi, gemini", s),
        }
    }

    pub fn default_model(&self) -> &'static str {
        match self {
            Self::Codex => "gpt-5.2-codex",
            Self::Claude => "claude-opus-4-5-20251101",
            Self::Pi => "pi",
            Self::Gemini => "gemini-2.5-pro",
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
    harness: Harness,
    model: String,
    dangerous: bool,
    reasoning_effort: String,
}

impl Runner {
    pub fn new(harness: Harness, model: String, dangerous: bool, reasoning_effort: String) -> Self {
        Self {
            harness,
            model,
            dangerous,
            reasoning_effort,
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
            .arg("-m")
            .arg(&self.model)
            .arg("-c")
            .arg(format!("model_reasoning_effort=\"{}\"", self.reasoning_effort));

        if self.dangerous {
            cmd.arg("-c").arg("approval_policy=\"never\"")
                .arg("-c").arg("sandbox_mode=\"danger-full-access\"");
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
                let name = json.get("tool_name")
                    .or_else(|| json.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown");
                let input = json.get("tool_input")
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

        if self.dangerous {
            cmd.arg("--dangerously-skip-permissions");
        }

        cmd.arg("--model")
            .arg(&self.model)
            .arg("-p")
            .arg(prompt)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        let status = cmd.status().await?;
        if !status.success() {
            bail!("pi exited with status: {}", status);
        }
        Ok(())
    }

    async fn run_gemini(&self, prompt: &str) -> Result<()> {
        let mut cmd = Command::new("gemini");

        if self.dangerous {
            cmd.arg("--sandbox").arg("none");
        }

        cmd.arg("--model")
            .arg(&self.model)
            .arg("-p")
            .arg(prompt)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        let status = cmd.status().await?;
        if !status.success() {
            bail!("gemini exited with status: {}", status);
        }
        Ok(())
    }
}
