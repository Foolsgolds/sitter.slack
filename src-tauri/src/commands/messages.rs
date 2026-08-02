use tauri::State;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Resolves the target channel by name (case-insensitive) against the full
/// catalog server-side, so the frontend never needs channel IDs to post.
#[tauri::command]
pub async fn post_message(state: State<'_, AppState>, channel: String, text: String) -> AppResult<()> {
    let client = state
        .client
        .read()
        .unwrap()
        .clone()
        .ok_or(AppError::NotAuthenticated)?;

    let trimmed_text = text.trim();
    if trimmed_text.is_empty() {
        return Err(AppError::Slack("message text must not be empty".to_string()));
    }

    let channel_id = state
        .channel_catalog
        .read()
        .unwrap()
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(&channel))
        .map(|c| c.channel_id.clone())
        .ok_or_else(|| AppError::Slack(format!("channel not found: {channel}")))?;

    client.post_message(&channel_id, trimmed_text).await
}
