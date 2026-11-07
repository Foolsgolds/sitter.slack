use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::db::{self, store, view};
use crate::error::{AppError, AppResult};
use crate::slack::api::{HistoryPage, SlackApi};
use crate::slack::dto::{SlackConversation, SlackTopicOrPurpose, SlackUser, SlackUserProfile};

use super::*;

// ------------------------------------------------------------------ fake Slack

#[derive(Default)]
struct FakeState {
    channels: Vec<SlackConversation>,
    emoji: HashMap<String, String>,
    /// Top-level messages per channel, in any order.
    history: HashMap<String, Vec<Value>>,
    /// Replies per (channel, parent ts).
    replies: HashMap<(String, String), Vec<Value>>,
    users: HashMap<String, SlackUser>,
    /// Every API call, as "method args", in order.
    calls: Vec<String>,
    fail_history: Option<String>,
}

#[derive(Default)]
struct FakeSlack {
    state: Mutex<FakeState>,
}

impl FakeSlack {
    fn with<R>(&self, f: impl FnOnce(&mut FakeState) -> R) -> R {
        f(&mut self.state.lock().unwrap())
    }

    fn calls(&self, prefix: &str) -> Vec<String> {
        self.with(|s| s.calls.iter().filter(|c| c.starts_with(prefix)).cloned().collect())
    }

    fn add_channel(&self, id: &str, name: &str) {
        self.with(|s| {
            s.channels.push(SlackConversation {
                id: id.to_string(),
                name: name.to_string(),
                is_member: true,
                num_members: Some(3),
                topic: Some(SlackTopicOrPurpose { value: "topic".into() }),
                ..Default::default()
            });
        });
    }

    fn add_user(&self, id: &str) {
        self.with(|s| {
            s.users.insert(
                id.to_string(),
                SlackUser {
                    id: id.to_string(),
                    name: format!("name-{id}"),
                    profile: SlackUserProfile {
                        display_name: format!("display-{id}"),
                        image_72: Some(format!("https://avatars/{id}-72.png")),
                        image_48: Some(format!("https://avatars/{id}-48.png")),
                        ..Default::default()
                    },
                },
            );
        });
    }

    fn put_message(&self, channel: &str, message: Value) {
        self.with(|s| {
            let list = s.history.entry(channel.to_string()).or_default();
            let ts = message["ts"].as_str().unwrap().to_string();
            list.retain(|m| m["ts"] != ts.as_str());
            list.push(message);
        });
    }
}

impl SlackApi for FakeSlack {
    async fn list_channels_page(&self, cursor: Option<&str>) -> AppResult<(Vec<SlackConversation>, Option<String>)> {
        // Two channels per page, to exercise the cursor loop.
        self.with(|s| {
            s.calls.push(format!("conversations.list cursor={cursor:?}"));
            let start: usize = cursor.map(|c| c.parse().unwrap()).unwrap_or(0);
            let end = (start + 2).min(s.channels.len());
            let next = (end < s.channels.len()).then(|| end.to_string());
            Ok((s.channels[start..end].to_vec(), next))
        })
    }

    async fn emoji_list(&self) -> AppResult<HashMap<String, String>> {
        self.with(|s| {
            s.calls.push("emoji.list".to_string());
            Ok(s.emoji.clone())
        })
    }

    async fn history_page(&self, channel_id: &str, cursor: Option<&str>, limit: u32) -> AppResult<HistoryPage> {
        self.with(|s| {
            s.calls.push(format!("conversations.history {channel_id} cursor={cursor:?}"));
            if let Some(error) = &s.fail_history {
                return Err(AppError::Slack(error.clone()));
            }
            let mut all = s.history.get(channel_id).cloned().unwrap_or_default();
            all.sort_by(|a, b| cmp_ts(b["ts"].as_str().unwrap(), a["ts"].as_str().unwrap()));
            let start: usize = cursor.map(|c| c.parse().unwrap()).unwrap_or(0);
            let end = (start + limit as usize).min(all.len());
            let has_more = end < all.len();
            Ok(HistoryPage {
                messages: all[start..end].to_vec(),
                has_more,
                next_cursor: has_more.then(|| end.to_string()),
            })
        })
    }

    async fn replies(&self, channel_id: &str, thread_ts: &str) -> AppResult<Vec<Value>> {
        self.with(|s| {
            s.calls.push(format!("conversations.replies {channel_id} {thread_ts}"));
            Ok(s.replies
                .get(&(channel_id.to_string(), thread_ts.to_string()))
                .cloned()
                .unwrap_or_default())
        })
    }

    async fn user_info(&self, user_id: &str) -> AppResult<SlackUser> {
        self.with(|s| {
            s.calls.push(format!("users.info {user_id}"));
            s.users
                .get(user_id)
                .cloned()
                .ok_or_else(|| AppError::Slack("user_not_found".to_string()))
        })
    }
}

// --------------------------------------------------------------------- helpers

fn ts(n: u32) -> String {
    format!("17000000{n:02}.000100")
}

fn message(n: u32, user: &str, text: &str) -> Value {
    json!({ "type": "message", "user": user, "ts": ts(n), "text": text })
}

fn thread_parent(n: u32, user: &str, reply_count: u32, latest_reply: u32) -> Value {
    json!({
        "type": "message", "user": user, "ts": ts(n), "text": "parent",
        "thread_ts": ts(n), "reply_count": reply_count, "latest_reply": ts(latest_reply)
    })
}

fn reply(n: u32, parent: u32, user: &str) -> Value {
    json!({ "type": "message", "user": user, "ts": ts(n), "text": "reply", "thread_ts": ts(parent) })
}

struct Fixture {
    fake: Arc<FakeSlack>,
    pool: SqlitePool,
    service: SyncService<FakeSlack>,
}

async fn fixture(limit: u32) -> Fixture {
    let fake = Arc::new(FakeSlack::default());
    fake.add_channel("C1", "times_alice");
    fake.add_channel("C2", "general");
    fake.add_channel("C3", "times_bob");
    fake.add_user("U1");
    fake.add_user("U2");
    let pool = db::connect_memory().await.unwrap();
    let config = SyncConfig {
        history_limit: limit,
        request_spacing: Duration::ZERO,
        ..Default::default()
    };
    let service = SyncService::new(pool.clone(), fake.clone(), config);
    service.sync_catalog().await.unwrap();
    Fixture { fake, pool, service }
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

// ------------------------------------------------------------------- catalog

#[tokio::test]
async fn catalog_sync_pages_stores_channels_emoji_and_prefix_targets() {
    let fx = fixture(15).await;
    fx.fake.with(|s| {
        s.emoji.insert("party".into(), "https://e/party.png".into());
        s.emoji.insert("celebrate".into(), "alias:party".into());
    });
    let outcome = fx.service.sync_catalog().await.unwrap();
    assert_eq!(outcome.channels, 3);
    assert_eq!(outcome.emoji, 2);

    // 3 channels at 2 per page -> two conversations.list calls per catalog sync.
    assert_eq!(fx.fake.calls("conversations.list").len(), 4);

    let catalog = store::list_catalog(&fx.pool).await.unwrap();
    assert_eq!(catalog.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["general", "times_alice", "times_bob"]);

    let emoji = view::emoji_map(&fx.pool).await.unwrap();
    assert_eq!(emoji["celebrate"], "https://e/party.png");

    let targets = fx.service.sync_targets().await.unwrap();
    let mut ids: Vec<_> = targets.iter().map(|t| t.channel_id.as_str()).collect();
    ids.sort();
    assert_eq!(ids, ["C1", "C3"]);
    assert!(targets.iter().all(|t| t.source == "prefix"));
}

#[tokio::test]
async fn catalog_sync_archives_channels_that_disappeared_and_drops_their_prefix_target() {
    let fx = fixture(15).await;
    fx.fake.with(|s| s.channels.retain(|c| c.id != "C3"));
    fx.service.sync_catalog().await.unwrap();

    let catalog = store::list_catalog(&fx.pool).await.unwrap();
    assert!(catalog.iter().all(|c| c.channel_id != "C3"));
    let archived: i64 = sqlx::query_scalar("SELECT is_archived FROM channels WHERE id = 'C3'")
        .fetch_one(&fx.pool)
        .await
        .unwrap();
    assert_eq!(archived, 1);
    let targets = fx.service.sync_targets().await.unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].channel_id, "C1");
}

#[tokio::test]
async fn custom_targets_survive_catalog_syncs_and_can_be_replaced() {
    let fx = fixture(15).await;
    fx.service.set_custom_targets(&["C2".to_string()]).await.unwrap();
    fx.service.sync_catalog().await.unwrap();

    let sources: HashMap<String, String> = fx
        .service
        .sync_targets()
        .await
        .unwrap()
        .into_iter()
        .map(|t| (t.channel_id, t.source))
        .collect();
    assert_eq!(sources["C2"], "custom");
    assert_eq!(sources["C1"], "prefix");

    fx.service.set_custom_targets(&[]).await.unwrap();
    assert_eq!(fx.service.sync_targets().await.unwrap().len(), 2);
}

// ----------------------------------------------------------------- idempotence

#[tokio::test]
async fn replaying_the_same_responses_does_not_add_rows() {
    let fx = fixture(15).await;
    fx.fake.put_message("C1", {
        let mut m = thread_parent(2, "U1", 1, 3);
        m["reactions"] = json!([{ "name": "tada", "count": 2, "users": ["U1", "U2"] }]);
        m["files"] = json!([{ "id": "F1", "mimetype": "image/png", "thumb_360": "https://f/360.png" }]);
        m
    });
    fx.fake.with(|s| {
        s.replies.insert(("C1".into(), ts(2)), vec![reply(3, 2, "U2")]);
    });

    fx.service.sync_channel("C1").await.unwrap();
    let after_first = (
        count(&fx.pool, "messages").await,
        count(&fx.pool, "reactions").await,
        count(&fx.pool, "files").await,
        count(&fx.pool, "users").await,
    );
    assert_eq!(after_first, (2, 1, 1, 2));

    // Same data again, plus a forced re-read of the whole channel.
    fx.service.sync_channel("C1").await.unwrap();
    store::record_sync_success(&fx.pool, "C1", None, 0).await.unwrap();
    fx.service.sync_channel("C1").await.unwrap();

    let after_replay = (
        count(&fx.pool, "messages").await,
        count(&fx.pool, "reactions").await,
        count(&fx.pool, "files").await,
        count(&fx.pool, "users").await,
    );
    assert_eq!(after_replay, after_first);
}

#[tokio::test]
async fn removed_reactions_are_dropped_on_the_next_read() {
    let fx = fixture(15).await;
    let mut with_reactions = message(1, "U1", "hi");
    with_reactions["reactions"] = json!([
        { "name": "tada", "count": 1, "users": ["U1"] },
        { "name": "eyes", "count": 1, "users": ["U2"] }
    ]);
    fx.fake.put_message("C1", with_reactions);
    fx.service.sync_channel("C1").await.unwrap();
    assert_eq!(count(&fx.pool, "reactions").await, 2);

    let mut fewer = message(1, "U1", "hi");
    fewer["reactions"] = json!([{ "name": "tada", "count": 3, "users": ["U1"] }]);
    fx.fake.put_message("C1", fewer);
    fx.service.sync_channel("C1").await.unwrap();

    let rows: Vec<(String, i64)> = sqlx::query_as("SELECT name, count FROM reactions")
        .fetch_all(&fx.pool)
        .await
        .unwrap();
    assert_eq!(rows, [("tada".to_string(), 3)]);
}

// ------------------------------------------------------------ incremental sync

#[tokio::test]
async fn first_sync_reads_one_page_and_later_syncs_page_back_only_to_the_stored_ts() {
    let fx = fixture(2).await;
    for n in 1..=3 {
        fx.fake.put_message("C1", message(n, "U1", "old"));
    }
    let first = fx.service.sync_channel("C1").await.unwrap();
    assert_eq!(first.pages, 1);
    assert_eq!(fx.fake.calls("conversations.history").len(), 1);
    assert_eq!(count(&fx.pool, "messages").await, 2); // ts 3 and 2
    assert_eq!(store::get_sync_state(&fx.pool, "C1").await.unwrap().last_message_ts, Some(ts(3)));

    for n in 4..=8 {
        fx.fake.put_message("C1", message(n, "U1", "new"));
    }
    let second = fx.service.sync_channel("C1").await.unwrap();
    // [8,7] -> [6,5] -> [4,3]; 3 is already stored, so paging stops there.
    assert_eq!(second.pages, 3);
    assert_eq!(fx.fake.calls("conversations.history").len(), 4);
    assert_eq!(count(&fx.pool, "messages").await, 7); // 2..=8
    assert_eq!(store::get_sync_state(&fx.pool, "C1").await.unwrap().last_message_ts, Some(ts(8)));

    // Nothing new: a single call re-reads the newest page and stops.
    let third = fx.service.sync_channel("C1").await.unwrap();
    assert_eq!(third.pages, 1);
    assert_eq!(fx.fake.calls("conversations.history").len(), 5);
}

#[tokio::test]
async fn paging_stops_at_the_page_limit() {
    let fx = fixture(1).await;
    fx.fake.put_message("C1", message(1, "U1", "seed"));
    fx.service.sync_channel("C1").await.unwrap();
    for n in 2..=9 {
        fx.fake.put_message("C1", message(n, "U1", "new"));
    }
    let limited = SyncService::new(
        fx.pool.clone(),
        fx.fake.clone(),
        SyncConfig {
            history_limit: 1,
            max_pages_per_channel: 3,
            ..Default::default()
        },
    );
    let outcome = limited.sync_channel("C1").await.unwrap();
    assert_eq!(outcome.pages, 3);
}

#[tokio::test]
async fn replies_are_fetched_only_for_parents_whose_thread_changed() {
    let fx = fixture(15).await;
    fx.fake.put_message("C1", thread_parent(5, "U1", 1, 6));
    fx.fake.put_message("C1", message(4, "U1", "plain"));
    fx.fake.with(|s| {
        s.replies.insert(("C1".into(), ts(5)), vec![reply(6, 5, "U2")]);
    });

    fx.service.sync_channel("C1").await.unwrap();
    assert_eq!(fx.fake.calls("conversations.replies").len(), 1);

    // Parent comes back unchanged: no replies call.
    fx.service.sync_channel("C1").await.unwrap();
    assert_eq!(fx.fake.calls("conversations.replies").len(), 1);

    // A new reply arrives: reply_count / latest_reply move, replies are fetched once more.
    fx.fake.put_message("C1", thread_parent(5, "U1", 2, 7));
    fx.fake.with(|s| {
        s.replies.insert(("C1".into(), ts(5)), vec![reply(6, 5, "U2"), reply(7, 5, "U1")]);
    });
    fx.service.sync_channel("C1").await.unwrap();
    assert_eq!(fx.fake.calls("conversations.replies").len(), 2);
    fx.service.sync_channel("C1").await.unwrap();
    assert_eq!(fx.fake.calls("conversations.replies").len(), 2);

    assert_eq!(count(&fx.pool, "messages").await, 4); // plain, parent, 2 replies
}

#[tokio::test]
async fn users_info_is_called_only_for_unknown_or_stale_users() {
    let fx = fixture(15).await;
    fx.fake.put_message("C1", message(1, "U1", "a"));
    fx.service.sync_channel("C1").await.unwrap();
    assert_eq!(fx.fake.calls("users.info"), ["users.info U1"]);

    // U1 is known now; only the newcomer U2 is looked up.
    fx.fake.put_message("C1", message(2, "U2", "b"));
    fx.fake.put_message("C1", message(3, "U1", "c"));
    fx.service.sync_channel("C1").await.unwrap();
    assert_eq!(fx.fake.calls("users.info"), ["users.info U1", "users.info U2"]);

    // A copy older than 24h is fetched again.
    sqlx::query("UPDATE users SET fetched_at = ?1 WHERE id = 'U1'")
        .bind(db::now_unix() - 25 * 3600)
        .execute(&fx.pool)
        .await
        .unwrap();
    fx.service.sync_channel("C1").await.unwrap();
    assert_eq!(fx.fake.calls("users.info").len(), 3);
    assert_eq!(fx.fake.calls("users.info")[2], "users.info U1");
}

#[tokio::test]
async fn a_failed_sync_is_recorded_and_keeps_the_previous_progress() {
    let fx = fixture(15).await;
    fx.fake.put_message("C1", message(1, "U1", "a"));
    fx.service.sync_channel("C1").await.unwrap();
    let mut events = fx.service.subscribe();

    fx.fake.with(|s| s.fail_history = Some("missing_scope".into()));
    assert!(fx.service.sync_channel("C1").await.is_err());

    let state = store::get_sync_state(&fx.pool, "C1").await.unwrap();
    assert_eq!(state.last_error.as_deref(), Some("slack api error: missing_scope"));
    assert_eq!(state.last_message_ts, Some(ts(1)));
    assert!(state.last_synced_at.is_some());
    match events.try_recv().unwrap() {
        SyncEvent::Error { message } => assert!(message.contains("missing_scope")),
        other => panic!("unexpected event {other:?}"),
    }

    // Recovery clears the error.
    fx.fake.with(|s| s.fail_history = None);
    fx.service.sync_channel("C1").await.unwrap();
    assert_eq!(store::get_sync_state(&fx.pool, "C1").await.unwrap().last_error, None);
}

#[tokio::test]
async fn syncing_a_channel_missing_from_the_catalog_fails_cleanly() {
    let fx = fixture(15).await;
    let err = fx.service.sync_channel("CNOPE").await.unwrap_err();
    assert!(err.to_string().contains("not in the catalog"));
    assert!(store::get_sync_state(&fx.pool, "CNOPE").await.unwrap().last_error.is_some());
}

// ------------------------------------------------------------------- read side

#[tokio::test]
async fn channel_view_has_the_display_model_shape() {
    let fx = fixture(15).await;
    let mut parent = thread_parent(5, "U1", 1, 6);
    parent["text"] = json!("*hello* :tada: <https://example.com/x|link>");
    parent["reactions"] = json!([{ "name": "tada", "count": 2, "users": ["U1", "U2"] }]);
    parent["files"] = json!([{
        "id": "F1", "mimetype": "image/png",
        "thumb_360": "https://files/360.png", "url_private": "https://files/full.png"
    }]);
    fx.fake.put_message("C1", parent);
    fx.fake.put_message("C1", message(4, "U2", "older"));
    fx.fake.with(|s| {
        s.replies.insert(("C1".into(), ts(5)), vec![reply(6, 5, "U2")]);
    });
    fx.service.sync_channel("C1").await.unwrap();

    let channel = view::channel_with_messages(&fx.pool, "C1", 10, Some("https://ws.slack.com"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(channel.name, "times_alice");
    assert_eq!(channel.topic_value.as_deref(), Some("topic"));
    assert_eq!(channel.num_members, Some(3));
    assert!(channel.is_member);
    assert_eq!(channel.channel_uri.as_deref(), Some("https://ws.slack.com/archives/C1"));
    assert_eq!(channel.last_message_ts, Some(ts(5)));

    // Newest first; the reply is nested under its parent, not listed on its own.
    assert_eq!(channel.messages.len(), 2);
    let first = &channel.messages[0];
    assert_eq!(first.ts.as_deref(), Some(ts(5).as_str()));
    assert_eq!(first.user_avatar_uri.as_deref(), Some("https://avatars/U1-72.png"));
    assert_eq!(
        first.permalink_uri.as_deref(),
        Some("https://ws.slack.com/archives/C1/p1700000005000100")
    );
    assert!(first.segments.iter().any(|s| s.text == "tada"));
    assert_eq!(first.reactions.len(), 1);
    assert_eq!(first.reactions[0].count, 2);
    assert_eq!(first.images[0].candidate_urls, ["https://files/360.png", "https://files/full.png"]);
    assert_eq!(first.replies.len(), 1);
    assert_eq!(first.replies[0].user_avatar_uri.as_deref(), Some("https://avatars/U2-72.png"));
    assert_eq!(channel.messages[1].text.as_deref(), Some("older"));

    // The limit keeps the newest N.
    let limited = view::channel_with_messages(&fx.pool, "C1", 1, None).await.unwrap().unwrap();
    assert_eq!(limited.messages.len(), 1);
    assert_eq!(limited.channel_uri, None);

    // JSON keys are camelCase, same as the live path.
    let json = serde_json::to_value(&channel).unwrap();
    assert!(json["channelId"].is_string());
    assert!(json["messages"][0]["userAvatarUri"].is_string());
    assert!(json["messages"][0]["replies"][0]["ts"].is_string());

    assert!(view::channel_with_messages(&fx.pool, "CNOPE", 10, None).await.unwrap().is_none());
    let batch = view::channels_with_messages(&fx.pool, &["CNOPE".into(), "C3".into(), "C1".into()], 10, None)
        .await
        .unwrap();
    assert_eq!(batch.iter().map(|c| c.channel_id.as_str()).collect::<Vec<_>>(), ["C3", "C1"]);
}

#[tokio::test]
async fn user_profile_is_read_from_the_database() {
    let fx = fixture(15).await;
    assert!(view::user_profile(&fx.pool, "U1").await.unwrap().is_none());
    fx.service.fetch_user("U1").await.unwrap();
    let profile = view::user_profile(&fx.pool, "U1").await.unwrap().unwrap();
    assert_eq!(profile.user_name, "name-U1");
    assert_eq!(profile.display_name, "display-U1");
    assert_eq!(profile.image_url, "https://avatars/U1-72.png"); // no 192 stored -> falls back to 72
}

// ------------------------------------------------------------------- scheduler

#[tokio::test]
async fn a_cycle_syncs_every_target_and_reports_progress() {
    let fx = fixture(15).await;
    fx.fake.put_message("C1", message(1, "U1", "a"));
    fx.fake.put_message("C3", message(1, "U2", "b"));
    let scheduler = SyncScheduler::new(fx.service.clone());
    let mut events = fx.service.subscribe();

    let summary = scheduler.run_cycle(false).await.unwrap();
    assert_eq!(summary, CycleSummary { total: 2, failed: 0 });
    assert!(!scheduler.status().running);
    assert!(scheduler.status().last_finished_at.is_some());

    let mut seen = Vec::new();
    while let Ok(event) = events.try_recv() {
        seen.push(event);
    }
    assert_eq!(seen.first(), Some(&SyncEvent::Started { total: 2 }));
    assert_eq!(seen.last(), Some(&SyncEvent::Completed { failed: 0 }));
    let updated: Vec<_> = seen
        .iter()
        .filter_map(|e| match e {
            SyncEvent::ChannelUpdated { channel_id } => Some(channel_id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(updated, ["C1", "C3"]);
}

#[tokio::test]
async fn a_failing_channel_does_not_stop_the_cycle() {
    let fx = fixture(15).await;
    fx.fake.put_message("C3", message(1, "U2", "b"));
    let scheduler = SyncScheduler::new(fx.service.clone());
    // C1 is not synced because history fails for everyone...
    fx.fake.with(|s| s.fail_history = Some("boom".into()));
    let summary = scheduler.run_cycle(false).await.unwrap();
    assert_eq!(summary, CycleSummary { total: 2, failed: 2 });
    // ...and both were still attempted.
    assert_eq!(fx.fake.calls("conversations.history").len(), 2);
}

#[tokio::test]
async fn a_second_cycle_is_refused_while_one_is_running_and_calls_are_spaced() {
    let fx = fixture(15).await;
    // Real (short) time: pausing the clock lets tokio jump ahead while a sqlx worker
    // thread is still answering, which times the pool out.
    let spacing = Duration::from_millis(150);
    let service = SyncService::new(
        fx.pool.clone(),
        fx.fake.clone(),
        SyncConfig {
            request_spacing: spacing,
            ..Default::default()
        },
    );
    let scheduler = SyncScheduler::new(service);
    let started = tokio::time::Instant::now();

    let (first, second) = tokio::join!(scheduler.run_cycle(true), async {
        tokio::task::yield_now().await;
        scheduler.run_cycle(true).await
    });
    assert!(first.is_ok());
    assert!(matches!(second, Err(CycleError::AlreadyRunning)));
    // Two channels, one history call each: the second waits one spacing interval.
    assert!(started.elapsed() >= spacing);
}

#[tokio::test]
async fn the_loop_syncs_the_catalog_first_and_stops_on_shutdown() {
    let fake = Arc::new(FakeSlack::default());
    fake.add_channel("C1", "times_alice");
    fake.put_message("C1", message(1, "U1", "a"));
    fake.add_user("U1");
    let pool = db::connect_memory().await.unwrap();
    let service = SyncService::new(
        pool.clone(),
        fake.clone(),
        SyncConfig {
            request_spacing: Duration::ZERO,
            cycle_interval: Duration::from_millis(30),
            ..Default::default()
        },
    );
    let scheduler = Arc::new(SyncScheduler::new(service));

    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let runner = {
        let scheduler = scheduler.clone();
        async move {
            scheduler
                .run(async {
                    let _ = stopped.await;
                })
                .await
        }
    };
    let driver = async {
        // Let two passes run, then shut down.
        tokio::time::timeout(Duration::from_secs(10), async {
            while fake.calls("conversations.history").len() < 2 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("two sync passes within 10s");
        stop.send(()).unwrap();
    };
    tokio::join!(runner, driver);

    assert_eq!(count(&pool, "messages").await, 1);
    assert_eq!(fake.calls("conversations.list").len(), 1); // catalog once, not due again
    assert!(fake.calls("conversations.history").len() >= 2);
    assert!(scheduler.status().last_catalog_at.is_some());
}

// ------------------------------------------------- change detection / broadcasts

#[tokio::test]
async fn channel_updated_is_sent_only_when_something_changed() {
    let fx = fixture(15).await;
    fx.fake.put_message("C1", message(1, "U1", "hi"));
    let mut events = fx.service.subscribe();
    let updates = |events: &mut broadcast::Receiver<SyncEvent>| {
        let mut n = 0;
        while let Ok(event) = events.try_recv() {
            if matches!(event, SyncEvent::ChannelUpdated { .. }) {
                n += 1;
            }
        }
        n
    };

    let first = fx.service.sync_channel("C1").await.unwrap();
    assert_eq!((first.messages, updates(&mut events)), (1, 1));

    // The same newest page again: nothing is rewritten, nothing is announced.
    let again = fx.service.sync_channel("C1").await.unwrap();
    assert_eq!((again.messages, again.replies, updates(&mut events)), (0, 0, 0));

    // A new reaction on a message that is already stored counts as a change.
    let mut reacted = message(1, "U1", "hi");
    reacted["reactions"] = json!([{ "name": "tada", "count": 1, "users": ["U2"] }]);
    fx.fake.put_message("C1", reacted);
    let changed = fx.service.sync_channel("C1").await.unwrap();
    assert_eq!((changed.messages, updates(&mut events)), (1, 1));
    assert_eq!(count(&fx.pool, "reactions").await, 1);
}

#[tokio::test]
async fn thread_broadcasts_are_listed_in_the_channel_and_under_their_parent() {
    let fx = fixture(15).await;
    fx.fake.put_message("C1", thread_parent(2, "U1", 2, 4));
    let mut broadcast = reply(4, 2, "U2");
    broadcast["subtype"] = json!("thread_broadcast");
    fx.fake.put_message("C1", broadcast.clone());
    fx.fake.with(|s| {
        s.replies
            .insert(("C1".into(), ts(2)), vec![reply(3, 2, "U2"), broadcast.clone()]);
    });
    fx.service.sync_channel("C1").await.unwrap();

    let channel = view::channel_with_messages(&fx.pool, "C1", 10, None).await.unwrap().unwrap();
    // Listed on its own (newest first) like it was in conversations.history ...
    let listed: Vec<_> = channel.messages.iter().map(|m| m.ts.clone().unwrap()).collect();
    assert_eq!(listed, [ts(4), ts(2)]);
    // ... and still shown under the parent, next to the plain reply.
    let parent = channel.messages.iter().find(|m| m.ts.as_deref() == Some(ts(2).as_str())).unwrap();
    let replies: Vec<_> = parent.replies.iter().map(|m| m.ts.clone().unwrap()).collect();
    assert_eq!(replies, [ts(3), ts(4)]);
}
