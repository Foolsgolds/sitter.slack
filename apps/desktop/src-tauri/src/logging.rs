use std::fs;
use std::path::PathBuf;

use tauri::{AppHandle, Manager};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};

const LOG_FILE_PREFIX: &str = "slacksitter";
const LOG_FILE_SUFFIX: &str = "log";
const MAX_LOG_FILES: usize = 7;

fn log_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_log_dir().map_err(|e| e.to_string())
}

/// Initializes file-based tracing into the app's log directory, one file per
/// day with the last `MAX_LOG_FILES` kept.
/// Returns a guard that must be kept alive for the lifetime of the process
/// (dropping it stops the background writer thread).
pub fn init(app: &AppHandle) -> WorkerGuard {
    let log_dir = log_dir(app).expect("app_log_dir should be resolvable");
    fs::create_dir_all(&log_dir).ok();

    let file_appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .filename_suffix(LOG_FILE_SUFFIX)
        .max_log_files(MAX_LOG_FILES)
        .build(&log_dir)
        // Logging must never stop the app from starting.
        .unwrap_or_else(|_| tracing_appender::rolling::never(&log_dir, "slacksitter.log"));
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(false)
        .init();

    tracing::info!(?log_dir, "logging initialized");
    guard
}

/// The last `max_lines` lines of the most recently written log file.
pub fn read_recent(app: &AppHandle, max_lines: usize) -> Result<String, String> {
    let dir = log_dir(app)?;
    let newest = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(LOG_FILE_PREFIX))
        .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path);

    match newest {
        Some(path) => Ok(tail_lines(&fs::read_to_string(path).map_err(|e| e.to_string())?, max_lines)),
        None => Ok(String::new()),
    }
}

fn tail_lines(text: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(max_lines)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_keeps_only_the_last_lines() {
        assert_eq!(tail_lines("a\nb\nc\nd", 2), "c\nd");
    }

    #[test]
    fn tail_returns_everything_when_shorter_than_the_limit() {
        assert_eq!(tail_lines("a\nb", 10), "a\nb");
        assert_eq!(tail_lines("", 10), "");
    }
}
