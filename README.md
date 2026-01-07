# Ralph

A CLI agent harness for running AI coding agents in a loop.

Ralph wraps multiple AI CLI agents (Codex, Claude, Pi, Gemini) providing a unified interface with configuration file support, iteration control, and dangerous mode settings.

## Installation

### Quick Install (Recommended)

**macOS / Linux:**
```bash
curl -fsSL https://raw.githubusercontent.com/madhavajay/ralph/main/install.sh | bash
```

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/madhavajay/ralph/main/install.ps1 | iex
```

### Via Cargo

```bash
cargo install ralph
```

### From Source

```bash
git clone https://github.com/madhavajay/ralph
cd ralph
cargo build --release
```

## Installing CLI Agents

Ralph can help install the CLI agents it supports:

```bash
# List available agents and their install status
ralph providers

# Show the install command Ralph would run
ralph install --list

# Install a specific agent
ralph install codex    # Installs OpenAI Codex CLI
ralph install claude   # Installs Anthropic Claude Code
ralph install gemini   # Installs Google Gemini CLI
ralph install pi       # Installs Pi CLI

# Install all detected-missing agents
ralph install --all
```

Optional (not yet supported as harnesses):

```bash
ralph install aider
ralph install goose
```

Ralph automatically detects your package manager (brew, npm, cargo, pip/pipx, winget) and OS to use the appropriate install method.

## Usage

```bash
# Run with a task file
ralph TASK.md

# Run with an inline prompt
ralph "fix the bug in main.rs"

# Specify a harness
ralph -H claude "implement the feature"

# Run multiple iterations
ralph -n 5 TASK.md

# Run infinitely
ralph -n inf TASK.md

# Use a specific model
ralph -H codex -m gpt-4 "review the code"
```

## Harnesses

Ralph supports the following AI CLI agents:

| Harness | Command | Default Model | Default Provider |
|---------|---------|---------------|------------------|
| codex   | `codex` | gpt-5.2-codex | - |
| claude  | `claude`| claude-opus-4-5-20251101 | - |
| pi      | `pi`    | claude-opus-4-5 | anthropic |
| gemini  | `gemini`| gemini-3 | - |

List available harnesses:

```bash
ralph --list-harnesses
```

## Configuration

Ralph supports configuration via `.ralphrc` or `.ralphrc.toml` files. Configuration is searched in:

1. Current directory (`.ralphrc` or `.ralphrc.toml`)
2. Home directory (`~/.ralphrc` or `~/.ralphrc.toml`)

Generate an example config:

```bash
ralph --init > .ralphrc
```

Example configuration:

```toml
# Agent harness: codex, claude, pi, gemini
harness = "codex"

# Model to use (optional, defaults vary by harness)
# model = "gpt-5.2-codex"

# Default task file
task = "TASK.md"

# Number of iterations (number or "inf")
iterations = "1"

# Enable dangerous mode (skip permissions)
dangerous = true

# Reasoning effort for codex
reasoning_effort = "medium"

# Provider for pi harness (anthropic, openai, google, etc.)
# provider = "anthropic"
```

## CLI Options

```
Usage: ralph [OPTIONS] [TASK]

Arguments:
  [TASK]  Task file or prompt string

Options:
  -H, --harness <HARNESS>        Agent harness to use: codex, claude, pi, gemini
  -m, --model <MODEL>            Model to use (defaults vary by harness)
  -n, --iterations <ITERATIONS>  Number of iterations or 'inf' for infinite loop [default: 1]
      --dangerous                Enable dangerous mode (skip permissions, full sandbox access) [default: true]
      --safe                     Disable dangerous mode (require permissions)
      --reasoning <REASONING>    Model reasoning effort level (for codex) [default: medium]
      --provider <PROVIDER>      Provider for pi harness (anthropic, openai, google, etc.)
      --list-harnesses           List available harnesses and exit
      --init                     Generate example .ralphrc config file
  -h, --help                     Print help
  -V, --version                  Print version
```

### Tmux Sessions

When tmux is available, ralph runs in a tmux session by default. Session names include timestamp and PID, and ralph will auto-suffix if a name is already in use to avoid collisions.

## Environment Variables

All CLI options can be set via environment variables:

- `RALPH_HARNESS` - Default harness
- `RALPH_MODEL` - Default model
- `RALPH_ITERATIONS` - Default iteration count
- `RALPH_TASK` - Default task file
- `RALPH_DANGEROUS` - Enable dangerous mode
- `RALPH_REASONING` - Reasoning effort level
- `RALPH_PROVIDER` - Provider for pi harness

## Prerequisites

Ralph requires the corresponding CLI tool to be installed for each harness:

- **codex**: [OpenAI Codex CLI](https://github.com/openai/codex)
- **claude**: [Claude Code](https://github.com/anthropics/claude-code)
- **pi**: Pi CLI
- **gemini**: [Gemini CLI](https://github.com/google-gemini/gemini-cli)

Ralph checks for the presence of the CLI tool before running and will error if not found.

## Platform Support

Ralph supports Linux, macOS, and Windows.

## Development

### Running Tests

```bash
# Run unit and integration tests
./test.sh

# Run clippy
./clippy.sh
```

### Harness Integration Tests

Integration tests that run against real CLI harnesses are available but ignored by default (they require API keys and network access):

```bash
# Run all harness tests
./test-harness.sh

# Run tests for a specific harness
./test-harness.sh claude
./test-harness.sh codex
./test-harness.sh pi
./test-harness.sh gemini
```

Or run directly with cargo:

```bash
cargo test --test harness_integration -- --ignored --nocapture
```

## License

Apache-2.0
