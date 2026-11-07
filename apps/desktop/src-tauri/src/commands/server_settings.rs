use tauri::AppHandle;

use crate::error::AppResult;
use crate::persistence::server_settings::{self, ServerSettings};

/// The saved connection, or `None` when no server URL / API key is stored yet.
#[tauri::command]
pub fn load_server_settings(app: AppHandle) -> AppResult<Option<ServerSettings>> {
    server_settings::load(&app)
}

/// Stores the server URL (settings file) and the API key (OS credential store).
/// The key is never logged.
#[tauri::command]
pub fn save_server_settings(app: AppHandle, server_url: String, api_key: String) -> AppResult<()> {
    server_settings::save(&app, &server_url, &api_key)?;
    tracing::info!("server connection settings saved");
    Ok(())
}

#[tauri::command]
pub fn clear_server_settings(app: AppHandle) -> AppResult<()> {
    server_settings::clear(&app)?;
    tracing::info!("server connection settings cleared");
    Ok(())
}
