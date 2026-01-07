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
Usage: ralph [OPTIONS] [TASK] [COMMAND]

Commands:
  providers  List detected providers and their status
  usage      Show usage/quota information for providers
  install    Install CLI agents with available package managers
  ps         List and manage spawned agent processes
  kill       Kill spawned agent processes
  cleanup    Clean up stale process entries and discover orphans
  logs       View and manage logs
  monitor    Run monitor mode (outer agent watches inner agent)
  help       Print this message or the help of the given subcommand(s)

Arguments:
  [TASK]  Task file or prompt string

Options:
  -H, --harness <HARNESS>        Agent harness to use: codex, claude, pi, gemini
  -m, --model <MODEL>            Model to use (defaults vary by harness)
  -n, --iterations <ITERATIONS>  Number of iterations or 'inf' for infinite loop [default: 1]
      --dangerous                Enable dangerous mode (skip permissions) [default: true]
      --safe                     Disable dangerous mode (require permissions)
      --reasoning <REASONING>    Model reasoning effort level (for codex) [default: medium]
      --provider <PROVIDER>      Provider for pi harness (anthropic, openai, google, etc.)
      --list-harnesses           List available harnesses and exit
      --init                     Generate example .ralphrc config file
      --tmux                     Run in tmux session (default when tmux is available)
      --no-tmux                  Run in foreground without tmux
      --tmux-attach              Attach to tmux session after starting
      --usage-limit-daily <N>    Stop at daily usage percentage (0-100)
      --usage-limit-weekly <N>   Stop at weekly usage percentage (0-100)
      --fallback-harness <NAME>  Switch to this harness when usage limit reached
  -v, --verbose                  Verbose logging (-v for debug, -vv for trace)
      --log-stderr               Also log to stderr (in addition to log file)
      --log-file                 Show log file location
  -h, --help                     Print help
  -V, --version                  Print version
```

### Tmux Sessions

When tmux is available, ralph runs in a tmux session by default. Session names include timestamp and PID, and ralph will auto-suffix if a name is already in use to avoid collisions.

### Process Management

Ralph tracks spawned agent processes and provides commands to manage them:

```bash
# List all tracked processes
ralph ps

# List all processes including dead ones
ralph ps --all

# Kill all tracked processes
ralph kill --all

# Kill processes in a specific directory
ralph kill --dir /path/to/project

# Kill processes for a specific harness
ralph kill --harness codex

# Clean up stale process entries
ralph cleanup

# Discover orphaned agent processes
ralph cleanup --discover

# Kill discovered orphans
ralph cleanup --discover --kill-orphans
```

### Logging

Ralph logs to `~/.ralph/logs/` with daily rotation. Use `-v` for debug logging or `-vv` for trace logging.

```bash
# View recent logs
ralph logs

# View last 100 lines
ralph logs --lines 100

# Follow log output (like tail -f)
ralph logs -f

# Show log file path
ralph logs --path

# Clear all log files
ralph logs --clear
```

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

- **codex**: [OpenAI Codex CLI](https://github.com/openai/codex) - `npm install -g @openai/codex`
- **claude**: [Claude Code](https://github.com/anthropics/claude-code) - `npm install -g @anthropic-ai/claude-code`
- **pi**: [Pi Coding Agent](https://github.com/mariozechner/pi-coding-agent) - `npm install -g @mariozechner/pi-coding-agent`
- **gemini**: [Gemini CLI](https://github.com/google-gemini/gemini-cli) - `npm install -g @anthropic-ai/gemini-cli`

Ralph checks for the presence of the CLI tool before running and will error if not found.

### Optional: Usage Tracking

To see provider usage/quota information with `ralph usage`, install codexbar:

```bash
brew install codexbar/codexbar/codexbar
```

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
