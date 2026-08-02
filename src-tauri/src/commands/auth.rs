use tauri::State;

use crate::error::AppResult;
use crate::persistence::token;
use crate::slack::client::SlackClient;
use crate::slack::models::AuthResult;
use crate::state::AppState;

async fn authenticate_inner(state: &AppState, token_value: String) -> AppResult<AuthResult> {
    let client = SlackClient::new(token_value.clone());
    let (workspace_url, user_id) = client.auth_test().await?;

    let avatar = client
        .user_info(&user_id)
        .await
        .ok()
        .and_then(|user| user.profile.image_192.or(user.profile.image_72));

    *state.client.write().unwrap() = Some(client);
    *state.workspace_url.write().unwrap() = Some(workspace_url.clone());
    *state.current_user_id.write().unwrap() = Some(user_id.clone());
    state.user_image_cache.write().unwrap().clear();

    token::save_token(&token_value)?;

    Ok(AuthResult {
        workspace_url,
        user_id,
        current_user_avatar_uri: avatar,
    })
}

#[tauri::command]
pub async fn authenticate(state: State<'_, AppState>, token_value: String) -> AppResult<AuthResult> {
    authenticate_inner(&state, token_value).await
}

#[tauri::command]
pub async fn try_auto_login(state: State<'_, AppState>) -> AppResult<Option<AuthResult>> {
    let Some(saved_token) = token::load_token()? else {
        return Ok(None);
    };
    match authenticate_inner(&state, saved_token).await {
        Ok(result) => Ok(Some(result)),
        Err(_) => Ok(None),
    }
}

#[tauri::command]
pub async fn logout(state: State<'_, AppState>) -> AppResult<()> {
    *state.client.write().unwrap() = None;
    *state.workspace_url.write().unwrap() = None;
    *state.current_user_id.write().unwrap() = None;
    state.user_image_cache.write().unwrap().clear();
    state.emoji_map.write().unwrap().clear();
    state.channel_catalog.write().unwrap().clear();
    token::delete_token()?;
    Ok(())
}
