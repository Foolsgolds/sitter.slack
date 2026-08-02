use std::fs;

use tauri::{AppHandle, Manager};
use tracing_appender::non_blocking::WorkerGuard;

/// Initializes file-based tracing into the app's log directory.
/// Returns a guard that must be kept alive for the lifetime of the process
/// (dropping it stops the background writer thread).
pub fn init(app: &AppHandle) -> WorkerGuard {
    let log_dir = app
        .path()
        .app_log_dir()
        .expect("app_log_dir should be resolvable");
    fs::create_dir_all(&log_dir).ok();

    let file_appender = tracing_appender::rolling::never(&log_dir, "slacksitter.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();

    tracing::info!(?log_dir, "logging initialized");
    guard
}
