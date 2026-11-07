use tauri::AppHandle;

use slacksitter_core::error::{AppError, AppResult};
use crate::logging;

/// Enough history for the log viewer without shipping a whole day's file to the UI.
const RECENT_LOG_LINES: usize = 500;

#[tauri::command]
pub fn get_logs(app: AppHandle) -> AppResult<String> {
    logging::read_recent(&app, RECENT_LOG_LINES).map_err(AppError::Persistence)
}

/// Lets the UI record its own events (settings toggles, board edits, errors)
/// in the same log as the backend. Never put secrets in `message`.
#[tauri::command]
pub fn write_log(level: String, message: String) {
    match level.as_str() {
        "error" => tracing::error!("{message}"),
        "warn" => tracing::warn!("{message}"),
        _ => tracing::info!("{message}"),
    }
}
