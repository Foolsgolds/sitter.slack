use tauri::State;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Checks whether the current user has already reacted before deciding
/// add vs. remove — this is NOT a blind toggle call, matching the legacy
/// `ChannelCardView_ReactionRequested` flow.
#[tauri::command]
pub async fn toggle_reaction(
    state: State<'_, AppState>,
    channel_id: String,
    ts: String,
    name: String,
) -> AppResult<()> {
    let client = state
        .client
        .read()
        .unwrap()
        .clone()
        .ok_or(AppError::NotAuthenticated)?;
    let user_id = state
        .current_user_id
        .read()
        .unwrap()
        .clone()
        .ok_or(AppError::NotAuthenticated)?;

    let already_reacted = client.has_user_reaction(&channel_id, &ts, &name, &user_id).await?;
    if already_reacted {
        client.remove_reaction(&channel_id, &ts, &name).await
    } else {
        client.add_reaction(&channel_id, &ts, &name).await
    }
}
