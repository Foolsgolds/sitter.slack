//! Write side of the database. Every write is an upsert, so replaying the
//! same Slack response any number of times leaves the same rows.

use std::collections::{HashMap, HashSet};

use serde_json::Value;
use sqlx::{QueryBuilder, Sqlite, SqliteConnection, SqlitePool};

use crate::error::{AppError, AppResult};
use crate::slack::dto::{SlackConversation, SlackMessage, SlackUser};
use crate::slack::models::ChannelSummary;

pub const SOURCE_PREFIX: &str = "prefix";
pub const SOURCE_CUSTOM: &str = "custom";

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct SyncTarget {
    pub channel_id: String,
    pub source: String,
    pub added_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, sqlx::FromRow)]
pub struct SyncState {
    pub channel_id: String,
    pub last_message_ts: Option<String>,
    pub last_synced_at: Option<i64>,
    pub last_error: Option<String>,
}

/// What we remember of a stored thread parent, to decide whether its replies changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadMarker {
    pub reply_count: i64,
    pub latest_reply: Option<String>,
}

// ---------------------------------------------------------------- channels

/// Upserts every channel of a full `conversations.list` result and marks the
/// known channels that did not show up in it as archived.
pub async fn replace_catalog(pool: &SqlitePool, channels: &[SlackConversation], now: i64) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    let mut seen = HashSet::with_capacity(channels.len());
    for channel in channels {
        seen.insert(channel.id.as_str());
        sqlx::query(
            "INSERT INTO channels (id, name, topic, purpose, num_members, is_member, is_archived, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7) \
             ON CONFLICT(id) DO UPDATE SET \
               name = excluded.name, topic = excluded.topic, purpose = excluded.purpose, \
               num_members = excluded.num_members, is_member = excluded.is_member, \
               is_archived = 0, updated_at = excluded.updated_at",
        )
        .bind(&channel.id)
        .bind(&channel.name)
        .bind(channel.topic.as_ref().map(|t| t.value.as_str()))
        .bind(channel.purpose.as_ref().map(|p| p.value.as_str()))
        .bind(channel.num_members)
        .bind(channel.is_member)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    let known: Vec<String> = sqlx::query_scalar("SELECT id FROM channels WHERE is_archived = 0")
        .fetch_all(&mut *tx)
        .await?;
    for id in known.iter().filter(|id| !seen.contains(id.as_str())) {
        sqlx::query("UPDATE channels SET is_archived = 1, updated_at = ?2 WHERE id = ?1")
            .bind(id)
            .bind(now)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Non-archived channels, by name.
pub async fn list_catalog(pool: &SqlitePool) -> AppResult<Vec<ChannelSummary>> {
    let rows: Vec<(String, String, bool, bool)> = sqlx::query_as(
        "SELECT id, name, is_member, is_archived FROM channels WHERE is_archived = 0 ORDER BY name, id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(channel_id, name, is_member, is_archived)| ChannelSummary {
            channel_id,
            name,
            is_member,
            is_archived,
        })
        .collect())
}

pub async fn channel_exists(pool: &SqlitePool, channel_id: &str) -> AppResult<bool> {
    let found: Option<i64> = sqlx::query_scalar("SELECT 1 FROM channels WHERE id = ?1")
        .bind(channel_id)
        .fetch_optional(pool)
        .await?;
    Ok(found.is_some())
}

// ------------------------------------------------------------------- emoji

/// Replaces the whole emoji table with the (alias-resolved) map.
pub async fn replace_emoji(pool: &SqlitePool, emoji: &HashMap<String, String>) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM emoji").execute(&mut *tx).await?;
    for (name, url) in emoji {
        sqlx::query("INSERT INTO emoji (name, url) VALUES (?1, ?2) ON CONFLICT(name) DO UPDATE SET url = excluded.url")
            .bind(name)
            .bind(url)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

// ----------------------------------------------------------- sync targets

/// Recomputes the `prefix` targets: every non-archived channel whose name
/// starts with `prefix`. `custom` rows are left alone (a channel that is
/// custom stays custom).
pub async fn reconcile_prefix_targets(pool: &SqlitePool, prefix: &str, now: i64) -> AppResult<()> {
    let names: Vec<(String, String)> = sqlx::query_as("SELECT id, name FROM channels WHERE is_archived = 0")
        .fetch_all(pool)
        .await?;
    let wanted: HashSet<String> = names
        .into_iter()
        .filter(|(_, name)| name.starts_with(prefix))
        .map(|(id, _)| id)
        .collect();

    let mut tx = pool.begin().await?;
    let current: Vec<String> = sqlx::query_scalar("SELECT channel_id FROM sync_targets WHERE source = ?1")
        .bind(SOURCE_PREFIX)
        .fetch_all(&mut *tx)
        .await?;
    for stale in current.iter().filter(|id| !wanted.contains(id.as_str())) {
        sqlx::query("DELETE FROM sync_targets WHERE channel_id = ?1 AND source = ?2")
            .bind(stale)
            .bind(SOURCE_PREFIX)
            .execute(&mut *tx)
            .await?;
    }
    for id in &wanted {
        sqlx::query(
            "INSERT INTO sync_targets (channel_id, source, added_at) VALUES (?1, ?2, ?3) \
             ON CONFLICT(channel_id) DO NOTHING",
        )
        .bind(id)
        .bind(SOURCE_PREFIX)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Makes the `custom` targets exactly `channel_ids`. A channel that is
/// already a `prefix` target stays `prefix`.
pub async fn set_custom_targets(pool: &SqlitePool, channel_ids: &[String], now: i64) -> AppResult<()> {
    let wanted: HashSet<&str> = channel_ids.iter().map(String::as_str).collect();
    let mut tx = pool.begin().await?;
    let current: Vec<String> = sqlx::query_scalar("SELECT channel_id FROM sync_targets WHERE source = ?1")
        .bind(SOURCE_CUSTOM)
        .fetch_all(&mut *tx)
        .await?;
    for stale in current.iter().filter(|id| !wanted.contains(id.as_str())) {
        sqlx::query("DELETE FROM sync_targets WHERE channel_id = ?1 AND source = ?2")
            .bind(stale)
            .bind(SOURCE_CUSTOM)
            .execute(&mut *tx)
            .await?;
    }
    for id in &wanted {
        sqlx::query(
            "INSERT INTO sync_targets (channel_id, source, added_at) VALUES (?1, ?2, ?3) \
             ON CONFLICT(channel_id) DO NOTHING",
        )
        .bind(id)
        .bind(SOURCE_CUSTOM)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// All targets, oldest first (a stable round-robin order for the scheduler).
pub async fn list_targets(pool: &SqlitePool) -> AppResult<Vec<SyncTarget>> {
    Ok(sqlx::query_as::<_, SyncTarget>(
        "SELECT channel_id, source, added_at FROM sync_targets ORDER BY added_at, channel_id",
    )
    .fetch_all(pool)
    .await?)
}

// -------------------------------------------------------------- sync state

pub async fn get_sync_state(pool: &SqlitePool, channel_id: &str) -> AppResult<SyncState> {
    let state = sqlx::query_as::<_, SyncState>(
        "SELECT channel_id, last_message_ts, last_synced_at, last_error FROM sync_state WHERE channel_id = ?1",
    )
    .bind(channel_id)
    .fetch_optional(pool)
    .await?;
    Ok(state.unwrap_or_else(|| SyncState {
        channel_id: channel_id.to_string(),
        ..Default::default()
    }))
}

pub async fn list_sync_states(pool: &SqlitePool) -> AppResult<Vec<SyncState>> {
    Ok(sqlx::query_as::<_, SyncState>(
        "SELECT channel_id, last_message_ts, last_synced_at, last_error FROM sync_state ORDER BY channel_id",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn record_sync_success(
    pool: &SqlitePool,
    channel_id: &str,
    last_message_ts: Option<&str>,
    now: i64,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO sync_state (channel_id, last_message_ts, last_synced_at, last_error) \
         VALUES (?1, ?2, ?3, NULL) \
         ON CONFLICT(channel_id) DO UPDATE SET \
           last_message_ts = excluded.last_message_ts, last_synced_at = excluded.last_synced_at, \
           last_error = NULL",
    )
    .bind(channel_id)
    .bind(last_message_ts)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Keeps `last_message_ts` / `last_synced_at` as they were.
pub async fn record_sync_failure(pool: &SqlitePool, channel_id: &str, error: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO sync_state (channel_id, last_error) VALUES (?1, ?2) \
         ON CONFLICT(channel_id) DO UPDATE SET last_error = excluded.last_error",
    )
    .bind(channel_id)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------- messages

/// Upserts one message (top-level or reply) with its reactions and files.
/// `raw` is stored verbatim as `raw_json`. Messages without a `ts` are skipped.
/// Returns whether the message is new or differs from the stored copy (an
/// unchanged re-read touches nothing, so it reports `false`).
pub async fn upsert_message(
    conn: &mut SqliteConnection,
    channel_id: &str,
    raw: &Value,
    now: i64,
) -> AppResult<bool> {
    let message: SlackMessage = serde_json::from_value(raw.clone())
        .map_err(|e| AppError::Persistence(format!("unreadable slack message: {e}")))?;
    let Some(ts) = message.ts.as_deref() else {
        return Ok(false);
    };

    let written = sqlx::query(
        "INSERT INTO messages \
           (channel_id, ts, user_id, text, subtype, thread_ts, reply_count, latest_reply, raw_json, fetched_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
         ON CONFLICT(channel_id, ts) DO UPDATE SET \
           user_id = excluded.user_id, text = excluded.text, subtype = excluded.subtype, \
           thread_ts = excluded.thread_ts, reply_count = excluded.reply_count, \
           latest_reply = excluded.latest_reply, raw_json = excluded.raw_json, \
           fetched_at = excluded.fetched_at \
         WHERE messages.raw_json IS NOT excluded.raw_json",
    )
    .bind(channel_id)
    .bind(ts)
    .bind(message.user.as_deref())
    .bind(message.text.as_deref())
    .bind(message.subtype.as_deref())
    .bind(message.thread_ts.as_deref())
    .bind(message.reply_count)
    .bind(message.latest_reply.as_deref())
    .bind(raw.to_string())
    .bind(now)
    .execute(&mut *conn)
    .await?;
    if written.rows_affected() == 0 {
        // Identical to what we have, so its reactions and files are current too.
        return Ok(false);
    }

    // Reactions: upsert the current set, drop the ones that were removed.
    let mut names = Vec::new();
    for reaction in &message.reactions {
        let Some(name) = reaction.name.as_deref().filter(|n| !n.trim().is_empty()) else {
            continue;
        };
        names.push(name.to_string());
        sqlx::query(
            "INSERT INTO reactions (channel_id, ts, name, count, users_json) VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(channel_id, ts, name) DO UPDATE SET \
               count = excluded.count, users_json = excluded.users_json",
        )
        .bind(channel_id)
        .bind(ts)
        .bind(name)
        .bind(reaction.count)
        .bind(serde_json::to_string(&reaction.users).unwrap_or_else(|_| "[]".to_string()))
        .execute(&mut *conn)
        .await?;
    }
    let existing: Vec<String> = sqlx::query_scalar("SELECT name FROM reactions WHERE channel_id = ?1 AND ts = ?2")
        .bind(channel_id)
        .bind(ts)
        .fetch_all(&mut *conn)
        .await?;
    for gone in existing.iter().filter(|n| !names.contains(n)) {
        sqlx::query("DELETE FROM reactions WHERE channel_id = ?1 AND ts = ?2 AND name = ?3")
            .bind(channel_id)
            .bind(ts)
            .bind(gone)
            .execute(&mut *conn)
            .await?;
    }

    for file in &message.files {
        let Some(id) = file.id.as_deref().filter(|id| !id.is_empty()) else {
            continue;
        };
        sqlx::query(
            "INSERT INTO files (id, channel_id, ts, mimetype, thumb_64, thumb_80, thumb_160, thumb_360, \
               thumb_480, url_private, url_private_download) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) \
             ON CONFLICT(id) DO UPDATE SET \
               channel_id = excluded.channel_id, ts = excluded.ts, mimetype = excluded.mimetype, \
               thumb_64 = excluded.thumb_64, thumb_80 = excluded.thumb_80, thumb_160 = excluded.thumb_160, \
               thumb_360 = excluded.thumb_360, thumb_480 = excluded.thumb_480, \
               url_private = excluded.url_private, url_private_download = excluded.url_private_download",
        )
        .bind(id)
        .bind(channel_id)
        .bind(ts)
        .bind(file.mimetype.as_deref())
        .bind(file.thumb_64.as_deref())
        .bind(file.thumb_80.as_deref())
        .bind(file.thumb_160.as_deref())
        .bind(file.thumb_360.as_deref())
        .bind(file.thumb_480.as_deref())
        .bind(file.url_private.as_deref())
        .bind(file.url_private_download.as_deref())
        .execute(&mut *conn)
        .await?;
    }
    Ok(true)
}

/// Stored `(reply_count, latest_reply)` for the given message timestamps.
pub async fn thread_markers(
    pool: &SqlitePool,
    channel_id: &str,
    timestamps: &[String],
) -> AppResult<HashMap<String, ThreadMarker>> {
    let mut markers = HashMap::new();
    if timestamps.is_empty() {
        return Ok(markers);
    }
    let mut query: QueryBuilder<Sqlite> =
        QueryBuilder::new("SELECT ts, reply_count, latest_reply FROM messages WHERE channel_id = ");
    query.push_bind(channel_id).push(" AND ts IN (");
    let mut separated = query.separated(", ");
    for ts in timestamps {
        separated.push_bind(ts);
    }
    separated.push_unseparated(")");
    let rows: Vec<(String, i64, Option<String>)> = query.build_query_as().fetch_all(pool).await?;
    for (ts, reply_count, latest_reply) in rows {
        markers.insert(ts, ThreadMarker { reply_count, latest_reply });
    }
    Ok(markers)
}

// ------------------------------------------------------------------- users

pub async fn upsert_user(pool: &SqlitePool, user: &SlackUser, now: i64) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO users (id, name, display_name, real_name, title, status_text, \
           image_32, image_48, image_72, image_192, fetched_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) \
         ON CONFLICT(id) DO UPDATE SET \
           name = excluded.name, display_name = excluded.display_name, real_name = excluded.real_name, \
           title = excluded.title, status_text = excluded.status_text, image_32 = excluded.image_32, \
           image_48 = excluded.image_48, image_72 = excluded.image_72, image_192 = excluded.image_192, \
           fetched_at = excluded.fetched_at",
    )
    .bind(&user.id)
    .bind(&user.name)
    .bind(&user.profile.display_name)
    .bind(&user.profile.real_name)
    .bind(&user.profile.title)
    .bind(&user.profile.status_text)
    .bind(user.profile.image_32.as_deref())
    .bind(user.profile.image_48.as_deref())
    .bind(user.profile.image_72.as_deref())
    .bind(user.profile.image_192.as_deref())
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// The subset of `user_ids` that is not stored yet or was fetched before `stale_before`.
pub async fn users_needing_refresh(
    pool: &SqlitePool,
    user_ids: &[String],
    stale_before: i64,
) -> AppResult<Vec<String>> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut query: QueryBuilder<Sqlite> = QueryBuilder::new("SELECT id, fetched_at FROM users WHERE id IN (");
    let mut separated = query.separated(", ");
    for id in user_ids {
        separated.push_bind(id);
    }
    separated.push_unseparated(")");
    let rows: Vec<(String, i64)> = query.build_query_as().fetch_all(pool).await?;
    let fresh: HashSet<String> = rows
        .into_iter()
        .filter(|(_, fetched_at)| *fetched_at >= stale_before)
        .map(|(id, _)| id)
        .collect();
    Ok(user_ids.iter().filter(|id| !fresh.contains(*id)).cloned().collect())
}
