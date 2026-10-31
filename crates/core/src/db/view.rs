//! Read side: assembles the display models (`ChannelWithMessages` and
//! friends) from the database. Message bodies, images and reactions are
//! derived from the stored `raw_json`, so a parser fix needs no re-fetch.

use std::collections::HashMap;

use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use crate::error::AppResult;
use crate::slack::dto::SlackMessage;
use crate::slack::models::{
    build_channel_uri, build_message_display_item, ChannelWithMessages, MessageDisplayItem,
    SlackUserProfileInfo,
};

#[derive(sqlx::FromRow)]
struct ChannelRow {
    id: String,
    name: String,
    topic: Option<String>,
    purpose: Option<String>,
    num_members: Option<i64>,
    is_member: bool,
}

#[derive(sqlx::FromRow)]
struct MessageRow {
    ts: String,
    raw_json: String,
}

/// The channel with its newest `limit` top-level messages (newest first),
/// each carrying its stored thread replies (oldest first). `None` when the
/// channel is not in the catalog.
pub async fn channel_with_messages(
    pool: &SqlitePool,
    channel_id: &str,
    limit: u32,
    workspace_url: Option<&str>,
) -> AppResult<Option<ChannelWithMessages>> {
    let channel = sqlx::query_as::<_, ChannelRow>(
        "SELECT id, name, topic, purpose, num_members, is_member FROM channels WHERE id = ?1",
    )
    .bind(channel_id)
    .fetch_optional(pool)
    .await?;
    let Some(channel) = channel else {
        return Ok(None);
    };

    let parents = sqlx::query_as::<_, MessageRow>(
        "SELECT ts, raw_json FROM messages \
         WHERE channel_id = ?1 AND (thread_ts IS NULL OR thread_ts = ts) \
         ORDER BY ts DESC LIMIT ?2",
    )
    .bind(channel_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let mut replies_by_parent: HashMap<String, Vec<SlackMessage>> = HashMap::new();
    let mut parsed_parents = Vec::with_capacity(parents.len());
    for row in &parents {
        let message = parse_raw(&row.raw_json);
        if message.reply_count > 0 {
            let replies = sqlx::query_as::<_, MessageRow>(
                "SELECT ts, raw_json FROM messages \
                 WHERE channel_id = ?1 AND thread_ts = ?2 AND ts <> ?2 ORDER BY ts ASC",
            )
            .bind(channel_id)
            .bind(&row.ts)
            .fetch_all(pool)
            .await?;
            replies_by_parent.insert(
                row.ts.clone(),
                replies.iter().map(|r| parse_raw(&r.raw_json)).collect(),
            );
        }
        parsed_parents.push(message);
    }

    let mut user_ids: Vec<&str> = parsed_parents
        .iter()
        .chain(replies_by_parent.values().flatten())
        .filter_map(|m| m.user.as_deref())
        .collect();
    user_ids.sort_unstable();
    user_ids.dedup();
    let avatars = avatar_urls(pool, &user_ids).await?;
    let avatar_of = |message: &SlackMessage| {
        message
            .user
            .as_deref()
            .and_then(|id| avatars.get(id))
            .map(String::as_str)
    };

    let messages: Vec<MessageDisplayItem> = parsed_parents
        .iter()
        .map(|message| {
            let replies = message
                .ts
                .as_ref()
                .and_then(|ts| replies_by_parent.get(ts))
                .map(|replies| {
                    replies
                        .iter()
                        .map(|reply| {
                            build_message_display_item(reply, channel_id, workspace_url, avatar_of(reply), Vec::new())
                        })
                        .collect()
                })
                .unwrap_or_default();
            build_message_display_item(message, channel_id, workspace_url, avatar_of(message), replies)
        })
        .collect();

    let last_fetched_ts = messages.first().and_then(|m| m.ts.clone());
    Ok(Some(ChannelWithMessages {
        channel_id: channel.id.clone(),
        channel_uri: build_channel_uri(workspace_url, Some(&channel.id)),
        name: channel.name,
        topic_value: channel.topic,
        purpose_value: channel.purpose,
        num_members: channel.num_members,
        is_member: channel.is_member,
        last_message_ts: last_fetched_ts.clone(),
        last_fetched_ts,
        messages,
    }))
}

/// `channel_with_messages` for several channels, in the order requested.
/// Unknown channel ids are skipped.
pub async fn channels_with_messages(
    pool: &SqlitePool,
    channel_ids: &[String],
    limit: u32,
    workspace_url: Option<&str>,
) -> AppResult<Vec<ChannelWithMessages>> {
    let mut channels = Vec::with_capacity(channel_ids.len());
    for id in channel_ids {
        if let Some(channel) = channel_with_messages(pool, id, limit, workspace_url).await? {
            channels.push(channel);
        }
    }
    Ok(channels)
}

/// Profile as the user dialog shows it; `None` when the user is not stored.
pub async fn user_profile(pool: &SqlitePool, user_id: &str) -> AppResult<Option<SlackUserProfileInfo>> {
    let row: Option<(String, String, String, String, String, String, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT id, name, display_name, real_name, title, status_text, image_192, image_72, image_48 \
             FROM users WHERE id = ?1",
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(
        |(user_id, user_name, display_name, real_name, title, status_text, i192, i72, i48)| SlackUserProfileInfo {
            user_id,
            user_name,
            display_name,
            real_name,
            title,
            status_text,
            image_url: i192.or(i72).or(i48).unwrap_or_default(),
        },
    ))
}

/// Flattened custom-emoji map (name -> URL).
pub async fn emoji_map(pool: &SqlitePool) -> AppResult<HashMap<String, String>> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT name, url FROM emoji").fetch_all(pool).await?;
    Ok(rows.into_iter().collect())
}

/// Avatar preference order matches the live path: 72 -> 48 -> 32.
async fn avatar_urls(pool: &SqlitePool, user_ids: &[&str]) -> AppResult<HashMap<String, String>> {
    if user_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut query: QueryBuilder<Sqlite> =
        QueryBuilder::new("SELECT id, image_72, image_48, image_32 FROM users WHERE id IN (");
    let mut separated = query.separated(", ");
    for id in user_ids {
        separated.push_bind(*id);
    }
    separated.push_unseparated(")");
    let rows: Vec<(String, Option<String>, Option<String>, Option<String>)> =
        query.build_query_as().fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, i72, i48, i32)| Some((id, i72.or(i48).or(i32)?)))
        .collect())
}

fn parse_raw(raw_json: &str) -> SlackMessage {
    serde_json::from_str(raw_json).unwrap_or_default()
}
