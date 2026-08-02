use std::collections::HashMap;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{Mutex as AsyncMutex, Semaphore};

use crate::error::{AppError, AppResult};
use crate::slack::client::SlackClient;
use crate::slack::dto::SlackConversation;
use crate::slack::models::{
    build_channel_uri, build_message_display_item, ChannelSummary, ChannelWithMessages,
    SlackUserProfileInfo,
};
use crate::state::AppState;

/// Only channels whose name starts with this literal (case-sensitive) prefix
/// are loaded onto the boards, though the full catalog (all names) is kept
/// separately for the add-channel picker / message-post resolution.
const CHANNEL_NAME_PREFIX: &str = "times";
const MESSAGE_LIMIT: u32 = 10;
const MAX_CONCURRENT_CHANNEL_LOADS: usize = 10;

fn get_client(state: &AppState) -> AppResult<SlackClient> {
    state
        .client
        .read()
        .unwrap()
        .clone()
        .ok_or(AppError::NotAuthenticated)
}

#[tauri::command]
pub async fn refresh_workspace_data(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    match refresh_workspace_data_inner(&app, &state).await {
        Ok(()) => {
            app.emit("refresh-complete", ()).ok();
            Ok(())
        }
        Err(err) => {
            app.emit("refresh-error", err.to_string()).ok();
            Err(err)
        }
    }
}

async fn refresh_workspace_data_inner(app: &AppHandle, state: &AppState) -> AppResult<()> {
    let client = get_client(state)?;
    let workspace_url = state.workspace_url.read().unwrap().clone();
    let user_id = state.current_user_id.read().unwrap().clone();

    let emoji_future = client.emoji_list();
    let avatar_future = async {
        match &user_id {
            Some(uid) => client.user_info(uid).await.ok(),
            None => None,
        }
    };
    let (emoji_result, _current_user) = tokio::join!(emoji_future, avatar_future);
    if let Ok(raw_emoji) = emoji_result {
        *state.emoji_map.write().unwrap() = crate::slack::emoji::flatten_emoji_map(&raw_emoji);
    }

    let mut all_channels = Vec::new();
    let mut cursor: Option<String> = None;
    let empty_oldest_map = HashMap::new();

    loop {
        let (page, next_cursor) = client.list_channels_page(cursor.as_deref()).await?;
        let mut times_batch = Vec::new();
        for channel in page {
            if channel.is_archived {
                continue;
            }
            all_channels.push(ChannelSummary {
                channel_id: channel.id.clone(),
                name: channel.name.clone(),
                is_member: channel.is_member,
                is_archived: channel.is_archived,
            });
            if channel.name.starts_with(CHANNEL_NAME_PREFIX) {
                times_batch.push(channel);
            }
        }

        if !times_batch.is_empty() {
            let loaded =
                load_channel_batch(&client, &times_batch, workspace_url.as_deref(), &empty_oldest_map).await?;
            app.emit("channel-batch", &loaded)
                .map_err(|e| AppError::Slack(e.to_string()))?;
        }

        cursor = next_cursor;
        if cursor.is_none() {
            break;
        }
    }

    *state.channel_catalog.write().unwrap() = all_channels;
    Ok(())
}

/// Loads messages (+ thread replies + avatars) for a batch of channels, at
/// most `MAX_CONCURRENT_CHANNEL_LOADS` channels in flight at once.
async fn load_channel_batch(
    client: &SlackClient,
    channels: &[SlackConversation],
    workspace_url: Option<&str>,
    oldest_by_channel: &HashMap<String, String>,
) -> AppResult<Vec<ChannelWithMessages>> {
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_CHANNEL_LOADS));
    let avatar_cache = Arc::new(AsyncMutex::new(HashMap::<String, Option<String>>::new()));

    let tasks = channels.iter().map(|channel| {
        let semaphore = semaphore.clone();
        let avatar_cache = avatar_cache.clone();
        let client = client.clone();
        let channel = channel.clone();
        let oldest = oldest_by_channel.get(&channel.id).cloned();
        let workspace_url = workspace_url.map(|s| s.to_string());
        async move {
            let _permit = semaphore.acquire().await.unwrap();
            load_single_channel(&client, &channel, workspace_url.as_deref(), oldest.as_deref(), &avatar_cache)
                .await
        }
    });

    futures::future::join_all(tasks).await.into_iter().collect()
}

async fn load_single_channel(
    client: &SlackClient,
    channel: &SlackConversation,
    workspace_url: Option<&str>,
    oldest: Option<&str>,
    avatar_cache: &AsyncMutex<HashMap<String, Option<String>>>,
) -> AppResult<ChannelWithMessages> {
    let messages = client.conversation_history(&channel.id, oldest, MESSAGE_LIMIT).await?;

    let mut display_messages = Vec::with_capacity(messages.len());
    for message in &messages {
        let replies = if message.reply_count > 0 {
            match &message.ts {
                Some(ts) => client
                    .conversation_replies(&channel.id, ts)
                    .await
                    .unwrap_or_default(),
                None => Vec::new(),
            }
        } else {
            Vec::new()
        };

        let mut reply_items = Vec::with_capacity(replies.len());
        for reply in &replies {
            let avatar = resolve_avatar(client, reply.user.as_deref(), avatar_cache).await;
            reply_items.push(build_message_display_item(
                reply,
                &channel.id,
                workspace_url,
                avatar.as_deref(),
                Vec::new(),
            ));
        }

        let avatar = resolve_avatar(client, message.user.as_deref(), avatar_cache).await;
        display_messages.push(build_message_display_item(
            message,
            &channel.id,
            workspace_url,
            avatar.as_deref(),
            reply_items,
        ));
    }

    let last_fetched_ts = display_messages.first().and_then(|m| m.ts.clone());

    Ok(ChannelWithMessages {
        channel_id: channel.id.clone(),
        channel_uri: build_channel_uri(workspace_url, Some(&channel.id)),
        name: channel.name.clone(),
        topic_value: channel.topic.as_ref().map(|t| t.value.clone()),
        purpose_value: channel.purpose.as_ref().map(|p| p.value.clone()),
        num_members: channel.num_members,
        is_member: channel.is_member,
        last_message_ts: last_fetched_ts.clone(),
        last_fetched_ts,
        messages: display_messages,
    })
}

/// Preference order matches the legacy `GetUserImageUrlAsync`: 72 -> 48 -> 32.
/// Failed/negative lookups are cached too, to avoid retrying every render.
async fn resolve_avatar(
    client: &SlackClient,
    user_id: Option<&str>,
    cache: &AsyncMutex<HashMap<String, Option<String>>>,
) -> Option<String> {
    let user_id = user_id?;
    {
        let guard = cache.lock().await;
        if let Some(cached) = guard.get(user_id) {
            return cached.clone();
        }
    }
    let resolved = client
        .user_info(user_id)
        .await
        .ok()
        .and_then(|user| user.profile.image_72.or(user.profile.image_48).or(user.profile.image_32));
    cache.lock().await.insert(user_id.to_string(), resolved.clone());
    resolved
}

#[tauri::command]
pub async fn list_all_channels(state: State<'_, AppState>) -> AppResult<Vec<ChannelSummary>> {
    Ok(state.channel_catalog.read().unwrap().clone())
}

#[tauri::command]
pub async fn refresh_channel(state: State<'_, AppState>, channel_id: String) -> AppResult<ChannelWithMessages> {
    let client = get_client(&state)?;
    let workspace_url = state.workspace_url.read().unwrap().clone();
    let summary = state
        .channel_catalog
        .read()
        .unwrap()
        .iter()
        .find(|c| c.channel_id == channel_id)
        .cloned()
        .ok_or_else(|| AppError::Slack("channel not found in catalog".to_string()))?;

    let conversation = SlackConversation {
        id: summary.channel_id,
        name: summary.name,
        is_member: summary.is_member,
        is_archived: summary.is_archived,
        ..Default::default()
    };
    let avatar_cache = AsyncMutex::new(HashMap::new());
    load_single_channel(&client, &conversation, workspace_url.as_deref(), None, &avatar_cache).await
}

#[tauri::command]
pub async fn get_user_profile(state: State<'_, AppState>, user_id: String) -> AppResult<SlackUserProfileInfo> {
    let client = get_client(&state)?;
    let user = client.user_info(&user_id).await?;
    Ok(SlackUserProfileInfo {
        user_id: user.id,
        user_name: user.name,
        display_name: user.profile.display_name,
        real_name: user.profile.real_name,
        title: user.profile.title,
        status_text: user.profile.status_text,
        image_url: user
            .profile
            .image_192
            .or(user.profile.image_72)
            .or(user.profile.image_48)
            .unwrap_or_default(),
    })
}

#[tauri::command]
pub async fn get_emoji_map(state: State<'_, AppState>) -> AppResult<HashMap<String, String>> {
    Ok(state.emoji_map.read().unwrap().clone())
}

#[tauri::command]
pub fn get_logs(app: AppHandle) -> AppResult<String> {
    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|e| AppError::Persistence(e.to_string()))?;
    let path = log_dir.join("slacksitter.log");
    Ok(std::fs::read_to_string(path).unwrap_or_default())
}
