//! Differential sync from Slack into SQLite. `SyncService` does the work for
//! one catalog / one channel; `SyncScheduler` walks the targets on a timer.

use std::cmp::Ordering;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use sqlx::SqlitePool;
use tokio::sync::{broadcast, Mutex};
use tokio::time::Instant;

use crate::db::store::{self, SyncTarget};
use crate::db::now_unix;
use crate::error::{AppError, AppResult};
use crate::slack::api::SlackApi;
use crate::slack::emoji::flatten_emoji_map;

pub mod scheduler;

#[cfg(test)]
mod tests;

pub use scheduler::{CycleError, CycleSummary, SyncScheduler, SyncStatus};

#[derive(Debug, Clone)]
pub struct SyncConfig {
    /// Channels whose name starts with this become `prefix` sync targets.
    pub channel_prefix: String,
    /// `limit` of each `conversations.history` call.
    pub history_limit: u32,
    /// Most `conversations.history` pages read for one channel in one sync.
    pub max_pages_per_channel: u32,
    /// Minimum gap between `conversations.history` / `conversations.replies` calls made by the scheduler.
    pub request_spacing: Duration,
    /// Pause between two passes over all targets.
    pub cycle_interval: Duration,
    /// How often the channel list and emoji are re-read.
    pub catalog_interval: Duration,
    /// A stored user older than this is fetched again.
    pub user_refresh_after: Duration,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            channel_prefix: "times".to_string(),
            history_limit: 15,
            max_pages_per_channel: 10,
            request_spacing: Duration::from_secs(60),
            cycle_interval: Duration::from_secs(300),
            catalog_interval: Duration::from_secs(3600),
            user_refresh_after: Duration::from_secs(24 * 3600),
        }
    }
}

/// Progress notifications (the server turns these into SSE events).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncEvent {
    /// A pass over `total` targets began.
    Started { total: usize },
    /// `done` of `total` targets have been tried.
    Progress { done: usize, total: usize },
    /// New or changed messages for this channel are now in the database.
    ChannelUpdated { channel_id: String },
    /// The pass ended; `failed` targets could not be synced.
    Completed { failed: usize },
    /// A catalog or channel sync failed (the message never contains a token).
    Error { message: String },
}

/// Spaces out calls: `wait()` returns once `spacing` has passed since the previous `wait()` returned.
pub struct Pacer {
    spacing: Duration,
    next_allowed: Mutex<Option<Instant>>,
}

impl Pacer {
    pub fn new(spacing: Duration) -> Self {
        Self {
            spacing,
            next_allowed: Mutex::new(None),
        }
    }

    pub async fn wait(&self) {
        let mut next_allowed = self.next_allowed.lock().await;
        if let Some(at) = *next_allowed {
            tokio::time::sleep_until(at).await;
        }
        *next_allowed = Some(Instant::now() + self.spacing);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogOutcome {
    pub channels: usize,
    pub emoji: usize,
    pub targets: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChannelSyncOutcome {
    /// Top-level messages that are new or changed since the last sync.
    pub messages: usize,
    /// Thread replies that are new or changed.
    pub replies: usize,
    pub pages: u32,
}

pub struct SyncService<A> {
    pool: SqlitePool,
    api: Arc<A>,
    config: SyncConfig,
    events: broadcast::Sender<SyncEvent>,
}

impl<A> Clone for SyncService<A> {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            api: self.api.clone(),
            config: self.config.clone(),
            events: self.events.clone(),
        }
    }
}

impl<A: SlackApi> SyncService<A> {
    pub fn new(pool: SqlitePool, api: Arc<A>, config: SyncConfig) -> Self {
        let (events, _) = broadcast::channel(256);
        Self { pool, api, config, events }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn api(&self) -> &Arc<A> {
        &self.api
    }

    pub fn config(&self) -> &SyncConfig {
        &self.config
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SyncEvent> {
        self.events.subscribe()
    }

    pub fn emit(&self, event: SyncEvent) {
        // No subscriber is fine.
        let _ = self.events.send(event);
    }

    /// Re-reads the channel list and the emoji, and recomputes the `prefix` targets.
    pub async fn sync_catalog(&self) -> AppResult<CatalogOutcome> {
        let result = self.sync_catalog_inner().await;
        if let Err(err) = &result {
            tracing::error!("catalog sync failed: {err}");
            self.emit(SyncEvent::Error {
                message: format!("catalog sync failed: {err}"),
            });
        }
        result
    }

    async fn sync_catalog_inner(&self) -> AppResult<CatalogOutcome> {
        let mut channels = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let (page, next) = self.api.list_channels_page(cursor.as_deref()).await?;
            channels.extend(page);
            cursor = next;
            if cursor.is_none() {
                break;
            }
        }
        let now = now_unix();
        store::replace_catalog(&self.pool, &channels, now).await?;

        let emoji = flatten_emoji_map(&self.api.emoji_list().await?);
        store::replace_emoji(&self.pool, &emoji).await?;

        store::reconcile_prefix_targets(&self.pool, &self.config.channel_prefix, now).await?;
        let targets = store::list_targets(&self.pool).await?.len();
        tracing::info!(channels = channels.len(), emoji = emoji.len(), targets, "catalog synced");
        Ok(CatalogOutcome {
            channels: channels.len(),
            emoji: emoji.len(),
            targets,
        })
    }

    pub async fn sync_targets(&self) -> AppResult<Vec<SyncTarget>> {
        store::list_targets(&self.pool).await
    }

    /// Replaces the `custom` targets.
    pub async fn set_custom_targets(&self, channel_ids: &[String]) -> AppResult<()> {
        store::set_custom_targets(&self.pool, channel_ids, now_unix()).await
    }

    /// Syncs one channel right away (no pacing). Failures are recorded in
    /// `sync_state.last_error` and also returned.
    pub async fn sync_channel(&self, channel_id: &str) -> AppResult<ChannelSyncOutcome> {
        self.sync_channel_with(channel_id, None).await
    }

    /// Like `sync_channel`, but every history / replies call waits on `pacer` first.
    pub async fn sync_channel_paced(&self, channel_id: &str, pacer: &Pacer) -> AppResult<ChannelSyncOutcome> {
        self.sync_channel_with(channel_id, Some(pacer)).await
    }

    async fn sync_channel_with(&self, channel_id: &str, pacer: Option<&Pacer>) -> AppResult<ChannelSyncOutcome> {
        match self.sync_channel_inner(channel_id, pacer).await {
            Ok(outcome) => {
                if outcome.messages + outcome.replies > 0 {
                    self.emit(SyncEvent::ChannelUpdated {
                        channel_id: channel_id.to_string(),
                    });
                }
                Ok(outcome)
            }
            Err(err) => {
                tracing::warn!(%channel_id, "channel sync failed: {err}");
                if let Err(db_err) = store::record_sync_failure(&self.pool, channel_id, &err.to_string()).await {
                    tracing::error!(%channel_id, "could not record sync failure: {db_err}");
                }
                self.emit(SyncEvent::Error {
                    message: format!("sync of {channel_id} failed: {err}"),
                });
                Err(err)
            }
        }
    }

    async fn sync_channel_inner(&self, channel_id: &str, pacer: Option<&Pacer>) -> AppResult<ChannelSyncOutcome> {
        if !store::channel_exists(&self.pool, channel_id).await? {
            return Err(AppError::Slack(format!("channel {channel_id} is not in the catalog")));
        }
        let state = store::get_sync_state(&self.pool, channel_id).await?;
        let last_ts = state.last_message_ts.clone();

        // 1-2. Always read the newest page (so recent reactions, edits and new replies are
        // picked up too); keep paging back only until we reach a message we already have.
        let mut messages: Vec<Value> = Vec::new();
        let mut cursor: Option<String> = None;
        let mut pages = 0u32;
        loop {
            wait_for(pacer).await;
            let page = self
                .api
                .history_page(channel_id, cursor.as_deref(), self.config.history_limit)
                .await?;
            pages += 1;

            let oldest_in_page = page.messages.iter().filter_map(raw_ts).min_by(|a, b| cmp_ts(a, b));
            let reached_stored = match (&last_ts, oldest_in_page) {
                (Some(stored), Some(oldest)) => cmp_ts(oldest, stored) != Ordering::Greater,
                _ => true,
            };
            let wants_more = page.has_more && !reached_stored && page.next_cursor.is_some();
            cursor = page.next_cursor;
            messages.extend(page.messages);

            if !wants_more {
                break;
            }
            if pages >= self.config.max_pages_per_channel {
                tracing::warn!(%channel_id, pages, "stopped paging at the page limit; older new messages are skipped");
                break;
            }
        }

        // 4. Threads whose reply_count / latest_reply moved since we stored the parent.
        let timestamps: Vec<String> = messages.iter().filter_map(raw_ts).map(str::to_string).collect();
        let markers = store::thread_markers(&self.pool, channel_id, &timestamps).await?;
        let mut thread_replies: Vec<Value> = Vec::new();
        for message in &messages {
            let Some(ts) = raw_ts(message) else { continue };
            let reply_count = message.get("reply_count").and_then(Value::as_i64).unwrap_or(0);
            if reply_count <= 0 {
                continue;
            }
            let latest_reply = message.get("latest_reply").and_then(Value::as_str);
            let unchanged = markers
                .get(ts)
                .map(|m| m.reply_count == reply_count && m.latest_reply.as_deref() == latest_reply)
                .unwrap_or(false);
            if unchanged {
                continue;
            }
            wait_for(pacer).await;
            thread_replies.extend(self.api.replies(channel_id, ts).await?);
        }

        // 3. Store everything in one transaction, so a half-fetched thread never
        // makes the parent look up to date.
        let now = now_unix();
        let mut tx = self.pool.begin().await?;
        let mut outcome = ChannelSyncOutcome {
            pages,
            ..Default::default()
        };
        for message in &messages {
            if store::upsert_message(&mut tx, channel_id, message, now).await? {
                outcome.messages += 1;
            }
        }
        for reply in &thread_replies {
            if store::upsert_message(&mut tx, channel_id, reply, now).await? {
                outcome.replies += 1;
            }
        }
        tx.commit().await?;

        // 5. Authors we do not know yet (or know only from long ago).
        let mut user_ids: Vec<String> = messages
            .iter()
            .chain(thread_replies.iter())
            .filter_map(|m| m.get("user").and_then(Value::as_str))
            .map(str::to_string)
            .collect();
        user_ids.sort();
        user_ids.dedup();
        let stale_before = now - self.config.user_refresh_after.as_secs() as i64;
        for user_id in store::users_needing_refresh(&self.pool, &user_ids, stale_before).await? {
            if let Err(err) = self.fetch_user(&user_id).await {
                tracing::warn!(%user_id, "users.info failed: {err}");
            }
        }

        // 6. Remember how far we got.
        let newest_seen = timestamps.iter().map(String::as_str).max_by(|a, b| cmp_ts(a, b));
        let new_last = match (last_ts.as_deref(), newest_seen) {
            (Some(stored), Some(seen)) if cmp_ts(seen, stored) == Ordering::Greater => Some(seen),
            (Some(stored), _) => Some(stored),
            (None, seen) => seen,
        };
        store::record_sync_success(&self.pool, channel_id, new_last, now).await?;
        tracing::info!(%channel_id, messages = outcome.messages, replies = outcome.replies, pages, "channel synced");
        Ok(outcome)
    }

    /// Re-reads the one message with this `ts` and stores it (a reaction was toggled, say).
    /// Returns whether the message still exists.
    pub async fn refresh_message(&self, channel_id: &str, ts: &str) -> AppResult<bool> {
        let Some(message) = self.api.message_at(channel_id, ts).await? else {
            return Ok(false);
        };
        let mut conn = self.pool.acquire().await?;
        let changed = store::upsert_message(&mut conn, channel_id, &message, now_unix()).await?;
        drop(conn);
        if changed {
            self.emit(SyncEvent::ChannelUpdated {
                channel_id: channel_id.to_string(),
            });
        }
        Ok(true)
    }

    /// Adds the reaction when `user_id` has not reacted with `name` yet, removes it
    /// otherwise (the live message decides, not a blind toggle), then refreshes the
    /// stored message. Returns whether the user now has the reaction.
    pub async fn toggle_reaction(&self, channel_id: &str, ts: &str, name: &str, user_id: &str) -> AppResult<bool> {
        let message = self
            .api
            .message_at(channel_id, ts)
            .await?
            .ok_or_else(|| AppError::Slack("message_not_found".to_string()))?;
        let reacted = user_has_reacted(&message, name, user_id);
        if reacted {
            self.api.remove_reaction(channel_id, ts, name).await?;
        } else {
            self.api.add_reaction(channel_id, ts, name).await?;
        }
        if let Err(err) = self.refresh_message(channel_id, ts).await {
            tracing::warn!(%channel_id, %ts, "message refresh after reaction failed: {err}");
        }
        Ok(!reacted)
    }

    /// Posts `text` to the channel called `channel_name` (case-insensitive, resolved
    /// against the stored catalog). Returns the channel id.
    pub async fn post_message(&self, channel_name: &str, text: &str) -> AppResult<String> {
        let text = text.trim();
        if text.is_empty() {
            return Err(AppError::Slack("message text must not be empty".to_string()));
        }
        let channel_id = store::channel_id_by_name(&self.pool, channel_name.trim())
            .await?
            .ok_or_else(|| AppError::Slack(format!("channel not found: {channel_name}")))?;
        self.api.post_message(&channel_id, text).await?;
        Ok(channel_id)
    }

    /// `users.info` for one user, stored regardless of how fresh the stored copy is.
    pub async fn fetch_user(&self, user_id: &str) -> AppResult<()> {
        let user = self.api.user_info(user_id).await?;
        store::upsert_user(&self.pool, &user, now_unix()).await
    }
}

async fn wait_for(pacer: Option<&Pacer>) {
    if let Some(pacer) = pacer {
        pacer.wait().await;
    }
}

/// Whether `user_id` is among the users of the reaction called `name` on this raw message.
pub fn user_has_reacted(message: &Value, name: &str, user_id: &str) -> bool {
    message
        .get("reactions")
        .and_then(Value::as_array)
        .map(|reactions| {
            reactions.iter().any(|reaction| {
                reaction.get("name").and_then(Value::as_str) == Some(name)
                    && reaction
                        .get("users")
                        .and_then(Value::as_array)
                        .map(|users| users.iter().any(|u| u.as_str() == Some(user_id)))
                        .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn raw_ts(message: &Value) -> Option<&str> {
    message.get("ts").and_then(Value::as_str)
}

/// Orders Slack timestamps ("seconds.micros") numerically without going
/// through floating point.
pub(crate) fn cmp_ts(a: &str, b: &str) -> Ordering {
    fn parts(ts: &str) -> (u64, u64) {
        let (secs, micros) = ts.split_once('.').unwrap_or((ts, "0"));
        (secs.parse().unwrap_or(0), micros.parse().unwrap_or(0))
    }
    parts(a).cmp(&parts(b))
}
