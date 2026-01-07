use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "ralph")]
#[command(author = "Madhava Jay <madhava@openmined.org>")]
#[command(version)]
#[command(about = "A CLI agent harness for running AI coding agents", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Agent harness to use: codex, claude, pi, gemini
    #[arg(short = 'H', long, env = "RALPH_HARNESS", global = true)]
    pub harness: Option<String>,

    /// Model to use (defaults vary by harness)
    #[arg(short, long, env = "RALPH_MODEL", global = true)]
    pub model: Option<String>,

    /// Number of iterations or 'inf' for infinite loop
    #[arg(
        short = 'n',
        long,
        default_value = "1",
        env = "RALPH_ITERATIONS",
        global = true
    )]
    pub iterations: String,

    /// Task file or prompt string
    #[arg(env = "RALPH_TASK")]
    pub task: Option<String>,

    /// Enable dangerous mode (skip permissions, full sandbox access)
    #[arg(long, default_value = "true", env = "RALPH_DANGEROUS", global = true)]
    pub dangerous: bool,

    /// Disable dangerous mode (require permissions)
    #[arg(long, conflicts_with = "dangerous", global = true)]
    pub safe: bool,

    /// Model reasoning effort level (for codex)
    #[arg(long, default_value = "medium", env = "RALPH_REASONING", global = true)]
    pub reasoning: String,

    /// Provider for pi harness (e.g., anthropic, openai, google)
    #[arg(long, env = "RALPH_PROVIDER", global = true)]
    pub provider: Option<String>,

    /// List available harnesses and exit
    #[arg(long)]
    pub list_harnesses: bool,

    /// Generate example .ralphrc config file
    #[arg(long)]
    pub init: bool,

    // Tmux options
    /// Run in tmux session (default when tmux is available)
    #[arg(long, env = "RALPH_TMUX", global = true)]
    pub tmux: bool,

    /// Run in foreground without tmux
    #[arg(long, conflicts_with = "tmux", global = true)]
    pub no_tmux: bool,

    /// Attach to tmux session after starting
    #[arg(long, global = true)]
    pub tmux_attach: bool,

    /// Custom tmux session name
    #[arg(long, global = true)]
    pub tmux_session: Option<String>,

    // Usage limit options
    /// Stop at daily usage percentage (0-100)
    #[arg(long, env = "RALPH_USAGE_LIMIT_DAILY", global = true)]
    pub usage_limit_daily: Option<u8>,

    /// Stop at weekly usage percentage (0-100)
    #[arg(long, env = "RALPH_USAGE_LIMIT_WEEKLY", global = true)]
    pub usage_limit_weekly: Option<u8>,

    /// Check usage every N iterations (0 = disabled)
    #[arg(long, global = true)]
    pub usage_check_interval: Option<u32>,

    /// Switch to this harness when usage limit reached
    #[arg(long, global = true)]
    pub fallback_harness: Option<String>,

    /// Verbose logging (-v for debug, -vv for trace)
    #[arg(short = 'v', long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Also log to stderr (in addition to log file)
    #[arg(long, global = true)]
    pub log_stderr: bool,

    /// Show log file location
    #[arg(long)]
    pub log_file: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// List detected providers and their status
    Providers {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show usage/quota information for providers
    Usage {
        /// Provider name or 'all' for all providers
        #[arg(long, default_value = "all")]
        provider: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Run monitor mode (outer agent watches inner agent)
    Monitor {
        /// Harness for monitor (outer) agent
        #[arg(long, default_value = "claude", env = "RALPH_MONITOR_HARNESS")]
        monitor_harness: String,

        /// Model for monitor agent
        #[arg(long)]
        monitor_model: Option<String>,

        /// Check interval (e.g., 1m, 5m, 10m)
        #[arg(long, default_value = "5m", env = "RALPH_MONITOR_INTERVAL")]
        monitor_interval: String,

        /// Harness for inner agent
        #[arg(long, default_value = "codex")]
        inner_harness: String,

        /// Model for inner agent
        #[arg(long)]
        inner_model: Option<String>,

        /// Task for the inner agent
        #[arg()]
        task: Option<String>,
    },

    /// Install CLI agents with available package managers
    Install {
        /// Agent name to install
        #[arg()]
        agent: Option<String>,

        /// Install all missing agents
        #[arg(long, conflicts_with = "agent")]
        all: bool,

        /// Show install commands for each agent
        #[arg(long, conflicts_with = "agent")]
        list: bool,
    },

    /// List and manage spawned agent processes
    Ps {
        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Show dead/exited processes too
        #[arg(long)]
        all: bool,
    },

    /// Kill spawned agent processes
    Kill {
        /// Kill all tracked processes
        #[arg(long)]
        all: bool,

        /// Kill processes in a specific directory
        #[arg(long)]
        dir: Option<String>,

        /// Kill processes for a specific harness (codex, claude, etc)
        #[arg(long)]
        harness: Option<String>,

        /// Specific PID to kill
        #[arg()]
        pid: Option<u32>,
    },

    /// Clean up stale process entries and discover orphans
    Cleanup {
        /// Also discover and register orphaned agent processes
        #[arg(long)]
        discover: bool,

        /// Kill discovered orphans instead of just registering
        #[arg(long)]
        kill_orphans: bool,
    },

    /// View and manage logs
    Logs {
        /// Number of lines to tail (0 for all)
        #[arg(long, default_value = "50")]
        lines: usize,

        /// Follow log output (like tail -f)
        #[arg(short = 'f', long)]
        follow: bool,

        /// Show log file path only
        #[arg(long)]
        path: bool,

        /// Clear all log files
        #[arg(long)]
        clear: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_defaults() {
        let cli = Cli::try_parse_from(["ralph"]).unwrap();
        assert!(cli.harness.is_none());
        assert!(cli.model.is_none());
        assert_eq!(cli.iterations, "1");
        assert!(cli.task.is_none());
        assert!(cli.dangerous);
        assert!(!cli.safe);
        assert_eq!(cli.reasoning, "medium");
        assert!(!cli.list_harnesses);
        assert!(!cli.init);
        assert!(cli.command.is_none());
        assert_eq!(cli.verbose, 0);
        assert!(!cli.log_stderr);
        assert!(!cli.log_file);
    }

    #[test]
    fn test_cli_harness_short_flag() {
        let cli = Cli::try_parse_from(["ralph", "-H", "claude"]).unwrap();
        assert_eq!(cli.harness, Some("claude".to_string()));
    }

    #[test]
    fn test_cli_harness_long_flag() {
        let cli = Cli::try_parse_from(["ralph", "--harness", "gemini"]).unwrap();
        assert_eq!(cli.harness, Some("gemini".to_string()));
    }

    #[test]
    fn test_cli_model_flags() {
        let cli = Cli::try_parse_from(["ralph", "-m", "test-model"]).unwrap();
        assert_eq!(cli.model, Some("test-model".to_string()));

        let cli = Cli::try_parse_from(["ralph", "--model", "another-model"]).unwrap();
        assert_eq!(cli.model, Some("another-model".to_string()));
    }

    #[test]
    fn test_cli_iterations() {
        let cli = Cli::try_parse_from(["ralph", "-n", "5"]).unwrap();
        assert_eq!(cli.iterations, "5");

        let cli = Cli::try_parse_from(["ralph", "--iterations", "inf"]).unwrap();
        assert_eq!(cli.iterations, "inf");
    }

    #[test]
    fn test_cli_task_positional() {
        let cli = Cli::try_parse_from(["ralph", "TASK.md"]).unwrap();
        assert_eq!(cli.task, Some("TASK.md".to_string()));

        let cli = Cli::try_parse_from(["ralph", "fix the bug"]).unwrap();
        assert_eq!(cli.task, Some("fix the bug".to_string()));
    }

    #[test]
    fn test_cli_safe_flag() {
        let cli = Cli::try_parse_from(["ralph", "--safe"]).unwrap();
        assert!(cli.safe);
    }

    #[test]
    fn test_cli_reasoning_flag() {
        let cli = Cli::try_parse_from(["ralph", "--reasoning", "high"]).unwrap();
        assert_eq!(cli.reasoning, "high");
    }

    #[test]
    fn test_cli_list_harnesses_flag() {
        let cli = Cli::try_parse_from(["ralph", "--list-harnesses"]).unwrap();
        assert!(cli.list_harnesses);
    }

    #[test]
    fn test_cli_init_flag() {
        let cli = Cli::try_parse_from(["ralph", "--init"]).unwrap();
        assert!(cli.init);
    }

    #[test]
    fn test_cli_combined_flags() {
        let cli = Cli::try_parse_from([
            "ralph",
            "-H",
            "claude",
            "-m",
            "claude-sonnet-4-20250514",
            "-n",
            "3",
            "--reasoning",
            "high",
            "TASK.md",
        ])
        .unwrap();
        assert_eq!(cli.harness, Some("claude".to_string()));
        assert_eq!(cli.model, Some("claude-sonnet-4-20250514".to_string()));
        assert_eq!(cli.iterations, "3");
        assert_eq!(cli.reasoning, "high");
        assert_eq!(cli.task, Some("TASK.md".to_string()));
    }

    #[test]
    fn test_cli_providers_subcommand() {
        let cli = Cli::try_parse_from(["ralph", "providers"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Providers { json: false })
        ));
    }

    #[test]
    fn test_cli_providers_json() {
        let cli = Cli::try_parse_from(["ralph", "providers", "--json"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Providers { json: true })
        ));
    }

    #[test]
    fn test_cli_usage_subcommand() {
        let cli = Cli::try_parse_from(["ralph", "usage"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Usage {
                provider: _,
                json: false
            })
        ));
    }

    #[test]
    fn test_cli_usage_with_provider() {
        let cli = Cli::try_parse_from(["ralph", "usage", "--provider", "claude"]).unwrap();
        if let Some(Commands::Usage { provider, json }) = cli.command {
            assert_eq!(provider, "claude");
            assert!(!json);
        } else {
            panic!("Expected Usage command");
        }
    }

    #[test]
    fn test_cli_monitor_subcommand() {
        let cli = Cli::try_parse_from(["ralph", "monitor"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Monitor { .. })));
    }

    #[test]
    fn test_cli_monitor_with_options() {
        let cli = Cli::try_parse_from([
            "ralph",
            "monitor",
            "--monitor-harness",
            "gemini",
            "--inner-harness",
            "claude",
            "--monitor-interval",
            "10m",
        ])
        .unwrap();
        if let Some(Commands::Monitor {
            monitor_harness,
            inner_harness,
            monitor_interval,
            ..
        }) = cli.command
        {
            assert_eq!(monitor_harness, "gemini");
            assert_eq!(inner_harness, "claude");
            assert_eq!(monitor_interval, "10m");
        } else {
            panic!("Expected Monitor command");
        }
    }

    #[test]
    fn test_cli_tmux_flags() {
        let cli = Cli::try_parse_from(["ralph", "--tmux"]).unwrap();
        assert!(cli.tmux);
        assert!(!cli.no_tmux);

        let cli = Cli::try_parse_from(["ralph", "--no-tmux"]).unwrap();
        assert!(!cli.tmux);
        assert!(cli.no_tmux);
    }

    #[test]
    fn test_cli_usage_limit_flags() {
        let cli = Cli::try_parse_from([
            "ralph",
            "--usage-limit-daily",
            "80",
            "--usage-limit-weekly",
            "90",
        ])
        .unwrap();
        assert_eq!(cli.usage_limit_daily, Some(80));
        assert_eq!(cli.usage_limit_weekly, Some(90));
    }

    #[test]
    fn test_cli_install_subcommand() {
        let cli = Cli::try_parse_from(["ralph", "install", "codex"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Install {
                agent: Some(_),
                all: false,
                list: false
            })
        ));
    }

    #[test]
    fn test_cli_install_all_flag() {
        let cli = Cli::try_parse_from(["ralph", "install", "--all"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Install {
                agent: None,
                all: true,
                list: false
            })
        ));
    }

    #[test]
    fn test_cli_install_list_flag() {
        let cli = Cli::try_parse_from(["ralph", "install", "--list"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Install {
                agent: None,
                all: false,
                list: true
            })
        ));
    }
}
