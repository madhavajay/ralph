use anyhow::Result;
use std::path::PathBuf;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Get the log directory (defaults to ~/.ralph/logs)
pub fn log_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ralph")
        .join("logs")
}

/// Get the current log file path
pub fn current_log_file() -> PathBuf {
    let now = chrono::Local::now();
    // tracing-appender uses format: prefix.YYYY-MM-DD
    log_dir().join(format!("ralph.{}", now.format("%Y-%m-%d")))
}

/// Initialize logging with file and optional stderr output
/// Returns a guard that must be kept alive for the duration of the program
pub fn init_logging(verbosity: u8, log_to_stderr: bool) -> Result<WorkerGuard> {
    let log_dir = log_dir();
    std::fs::create_dir_all(&log_dir)?;

    // Create a file appender that rotates daily
    let file_appender = tracing_appender::rolling::daily(&log_dir, "ralph");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // Determine log level based on verbosity
    let level = match verbosity {
        0 => Level::INFO,
        1 => Level::DEBUG,
        _ => Level::TRACE,
    };

    // Build the filter
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level.to_string()));

    // File layer - always JSON for easy parsing
    let file_layer = fmt::layer()
        .json()
        .with_writer(non_blocking)
        .with_ansi(false);

    if log_to_stderr {
        // Also log to stderr with human-readable format
        let stderr_layer = fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(true)
            .compact();

        tracing_subscriber::registry()
            .with(filter)
            .with(file_layer)
            .with(stderr_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(file_layer)
            .init();
    }

    Ok(guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_dir() {
        let dir = log_dir();
        assert!(dir.to_string_lossy().contains(".ralph"));
        assert!(dir.to_string_lossy().contains("logs"));
    }

    #[test]
    fn test_current_log_file() {
        let file = current_log_file();
        // tracing-appender uses format: prefix.YYYY-MM-DD
        assert!(file.to_string_lossy().contains("ralph."));
    }
}
