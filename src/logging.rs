use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

static LOG_DIR: LazyLock<PathBuf> = LazyLock::new(resolve_log_dir);

/// Get the log directory (defaults to ~/.ralph/logs)
pub fn log_dir() -> PathBuf {
    LOG_DIR.clone()
}

fn resolve_log_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("RALPH_LOG_DIR") {
        let path = PathBuf::from(dir);
        if ensure_writable_dir(&path) {
            return path;
        }
    }

    if let Some(home) = dirs::home_dir() {
        let path = home.join(".ralph").join("logs");
        if ensure_writable_dir(&path) {
            return path;
        }
    }

    if let Ok(xdg_state) = std::env::var("XDG_STATE_HOME") {
        let path = PathBuf::from(xdg_state).join("ralph").join("logs");
        if ensure_writable_dir(&path) {
            return path;
        }
    }

    if let Ok(xdg_cache) = std::env::var("XDG_CACHE_HOME") {
        let path = PathBuf::from(xdg_cache).join("ralph").join("logs");
        if ensure_writable_dir(&path) {
            return path;
        }
    }

    std::env::temp_dir().join(".ralph").join("logs")
}

fn ensure_writable_dir(path: &Path) -> bool {
    if std::fs::create_dir_all(path).is_err() {
        return false;
    }

    let test_path = path.join(format!(".write-test-{}", std::process::id()));
    let result = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&test_path)
        .is_ok();
    let _ = std::fs::remove_file(&test_path);
    result
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
