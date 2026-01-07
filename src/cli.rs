use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "ralph")]
#[command(author = "Madhava Jay <madhava@openmined.org>")]
#[command(version)]
#[command(about = "A CLI agent harness for running AI coding agents", long_about = None)]
pub struct Cli {
    /// Agent harness to use: codex, claude, pi, gemini
    #[arg(short = 'H', long, env = "RALPH_HARNESS")]
    pub harness: Option<String>,

    /// Model to use (defaults vary by harness)
    #[arg(short, long, env = "RALPH_MODEL")]
    pub model: Option<String>,

    /// Number of iterations or 'inf' for infinite loop
    #[arg(short = 'n', long, default_value = "1", env = "RALPH_ITERATIONS")]
    pub iterations: String,

    /// Task file or prompt string
    #[arg(env = "RALPH_TASK")]
    pub task: Option<String>,

    /// Enable dangerous mode (skip permissions, full sandbox access)
    #[arg(long, default_value = "true", env = "RALPH_DANGEROUS")]
    pub dangerous: bool,

    /// Disable dangerous mode (require permissions)
    #[arg(long, conflicts_with = "dangerous")]
    pub safe: bool,

    /// Model reasoning effort level (for codex)
    #[arg(long, default_value = "medium", env = "RALPH_REASONING")]
    pub reasoning: String,

    /// List available harnesses and exit
    #[arg(long)]
    pub list_harnesses: bool,

    /// Generate example .ralphrc config file
    #[arg(long)]
    pub init: bool,
}
