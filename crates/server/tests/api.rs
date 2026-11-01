//! Drives the real router with `oneshot`, against an in-memory database and a
//! Slack that answers nothing (everything the API needs is already stored).

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use slacksitter_core::db::{self, store, view, SqlitePool};
use slacksitter_core::error::{AppError, AppResult};
use slacksitter_core::slack::api::{HistoryPage, SlackApi};
use slacksitter_core::slack::dto::{SlackConversation, SlackUser};
use slacksitter_core::slack::models::AuthResult;
use slacksitter_core::sync::{SyncConfig, SyncScheduler, SyncService};
use slacksitter_server::config::Secret;
use slacksitter_server::images::ImageProxy;
use slacksitter_server::routes::router;
use slacksitter_server::state::{AppState, Me};
use slacksitter_server::build_cors;
use tower::ServiceExt;

const API_KEY: &str = "test-api-key";

/// A Slack that fails every call.
struct NoSlack;

fn offline<T>() -> AppResult<T> {
    Err(AppError::Slack("offline".to_string()))
}

impl SlackApi for NoSlack {
    async fn list_channels_page(&self, _: Option<&str>) -> AppResult<(Vec<SlackConversation>, Option<String>)> {
        offline()
    }
    async fn emoji_list(&self) -> AppResult<HashMap<String, String>> {
        offline()
    }
    async fn history_page(&self, _: &str, _: Option<&str>, _: u32) -> AppResult<HistoryPage> {
        offline()
    }
    async fn replies(&self, _: &str, _: &str) -> AppResult<Vec<Value>> {
        offline()
    }
    async fn user_info(&self, _: &str) -> AppResult<SlackUser> {
        offline()
    }
    async fn post_message(&self, _: &str, _: &str) -> AppResult<()> {
        offline()
    }
    async fn add_reaction(&self, _: &str, _: &str, _: &str) -> AppResult<()> {
        offline()
    }
    async fn remove_reaction(&self, _: &str, _: &str, _: &str) -> AppResult<()> {
        offline()
    }
    async fn message_at(&self, _: &str, _: &str) -> AppResult<Option<Value>> {
        offline()
    }
}

struct TestApp {
    app: Router,
    pool: SqlitePool,
    service: SyncService<NoSlack>,
}

async fn test_app() -> TestApp {
    let pool = db::connect_memory().await.unwrap();
    let service = SyncService::new(pool.clone(), Arc::new(NoSlack), SyncConfig::default());
    let scheduler = Arc::new(SyncScheduler::new(service.clone()));
    let me = Me {
        auth: AuthResult {
            workspace_url: "https://ws.slack.com".to_string(),
            user_id: "UME".to_string(),
            current_user_avatar_uri: Some("https://avatars/me.png".to_string()),
        },
    };
    let state = AppState::new(
        Secret::new(API_KEY),
        scheduler,
        me,
        ImageProxy::new(Secret::new("xoxp-test")),
        std::env::temp_dir().join("slacksitter-no-logs-here"),
    );
    let cors = build_cors(&["http://localhost:1420".to_string()]).unwrap();
    TestApp {
        app: router(state, cors),
        pool,
        service,
    }
}

async fn seed(pool: &SqlitePool) {
    let channels: Vec<SlackConversation> = serde_json::from_value(json!([
        { "id": "C1", "name": "times_alice", "is_member": true, "num_members": 4,
          "topic": { "value": "hello" }, "purpose": { "value": "" } },
        { "id": "C2", "name": "general", "is_member": false }
    ]))
    .unwrap();
    store::replace_catalog(pool, &channels, 1).await.unwrap();
    let users: SlackUser = serde_json::from_value(json!({
        "id": "U1", "name": "alice", "profile": { "display_name": "Alice", "image_72": "https://avatars/u1.png" }
    }))
    .unwrap();
    store::upsert_user(pool, &users, 1).await.unwrap();

    let mut conn = pool.acquire().await.unwrap();
    store::upsert_message(
        &mut conn,
        "C1",
        &json!({
            "user": "U1", "ts": "1700000002.000100", "text": "*hi* :tada:",
            "thread_ts": "1700000002.000100", "reply_count": 1, "latest_reply": "1700000003.000100",
            "reactions": [{ "name": "tada", "count": 2, "users": ["U1", "UME"] }]
        }),
        1,
    )
    .await
    .unwrap();
    store::upsert_message(
        &mut conn,
        "C1",
        &json!({ "user": "U1", "ts": "1700000003.000100", "text": "reply", "thread_ts": "1700000002.000100" }),
        1,
    )
    .await
    .unwrap();
    store::upsert_message(&mut conn, "C1", &json!({ "user": "U1", "ts": "1700000001.000100", "text": "first" }), 1)
        .await
        .unwrap();
}

fn get(path: &str, key: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method(Method::GET).uri(path);
    if let Some(key) = key {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {key}"));
    }
    builder.body(Body::empty()).unwrap()
}

fn send_json(method: Method, path: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {API_KEY}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn call(app: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

#[tokio::test]
async fn health_needs_no_key_and_everything_else_does() {
    let t = test_app().await;

    let (status, body) = call(&t.app, get("/api/health", None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
    assert!(body["version"].is_string());

    for path in [
        "/api/me",
        "/api/channels",
        "/api/channels/C1",
        "/api/users/U1",
        "/api/emoji",
        "/api/images?url=https://example.com/a.png",
        "/api/sync/status",
        "/api/sync/targets",
        "/api/events",
        "/api/logs",
    ] {
        let (status, body) = call(&t.app, get(path, None)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path} without a key");
        assert!(body["error"].is_string());
        let (status, _) = call(&t.app, get(path, Some("wrong-key"))).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path} with a wrong key");
    }

    for (method, path) in [
        (Method::POST, "/api/channels/batch"),
        (Method::POST, "/api/messages"),
        (Method::POST, "/api/reactions/toggle"),
        (Method::POST, "/api/sync"),
        (Method::POST, "/api/sync/channels/C1"),
        (Method::POST, "/api/sync/channels"),
        (Method::PUT, "/api/sync/targets"),
    ] {
        let request = Request::builder().method(method).uri(path).body(Body::empty()).unwrap();
        let (status, _) = call(&t.app, request).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path} without a key");
    }

    let (status, _) = call(&t.app, get("/api/channels", Some(API_KEY))).await;
    assert_eq!(status, StatusCode::OK);
    // The scheme is case-insensitive, but it must be Bearer.
    let request = Request::builder()
        .uri("/api/channels")
        .header(header::AUTHORIZATION, format!("Basic {API_KEY}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(call(&t.app, request).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn me_reports_the_signed_in_user_and_the_channel_prefix() {
    let t = test_app().await;
    let (status, body) = call(&t.app, get("/api/me", Some(API_KEY))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({
            "workspaceUrl": "https://ws.slack.com",
            "userId": "UME",
            "currentUserAvatarUri": "https://avatars/me.png",
            "channelPrefix": "times"
        })
    );
}

#[tokio::test]
async fn the_catalog_lists_unarchived_channels() {
    let t = test_app().await;
    seed(&t.pool).await;
    let (status, body) = call(&t.app, get("/api/channels", Some(API_KEY))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!([
            { "channelId": "C2", "name": "general", "isMember": false, "isArchived": false },
            { "channelId": "C1", "name": "times_alice", "isMember": true, "isArchived": false }
        ])
    );
}

#[tokio::test]
async fn a_channel_has_the_channel_with_messages_shape() {
    let t = test_app().await;
    seed(&t.pool).await;

    let (status, body) = call(&t.app, get("/api/channels/C1", Some(API_KEY))).await;
    assert_eq!(status, StatusCode::OK);

    // Exactly the model the database layer builds, serialized the same way.
    let expected = view::channel_with_messages(&t.pool, "C1", 10, Some("https://ws.slack.com"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(body, serde_json::to_value(expected).unwrap());

    let keys: Vec<&str> = body.as_object().unwrap().keys().map(String::as_str).collect();
    for key in [
        "channelId", "channelUri", "name", "topicValue", "purposeValue", "numMembers", "isMember",
        "lastMessageTs", "lastFetchedTs", "messages",
    ] {
        assert!(keys.contains(&key), "missing {key}");
    }
    assert_eq!(body["channelUri"], "https://ws.slack.com/archives/C1");
    assert_eq!(body["topicValue"], "hello");
    assert_eq!(body["numMembers"], 4);
    assert_eq!(body["lastMessageTs"], "1700000002.000100");

    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2); // the reply is nested, not listed
    let newest = &messages[0];
    for key in ["user", "ts", "text", "userAvatarUri", "permalinkUri", "segments", "images", "reactions", "replies"] {
        assert!(newest.get(key).is_some(), "message is missing {key}");
    }
    assert_eq!(newest["userAvatarUri"], "https://avatars/u1.png");
    assert_eq!(newest["reactions"], json!([{ "name": "tada", "count": 2 }]));
    assert_eq!(newest["replies"][0]["ts"], "1700000003.000100");
    assert_eq!(newest["segments"][0]["segType"], "text");
    assert_eq!(newest["segments"][0]["isBold"], true);

    // ?limit= keeps the newest N top-level messages.
    let (_, body) = call(&t.app, get("/api/channels/C1?limit=1", Some(API_KEY))).await;
    assert_eq!(body["messages"].as_array().unwrap().len(), 1);

    let (status, body) = call(&t.app, get("/api/channels/CNOPE", Some(API_KEY))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn batch_returns_known_channels_in_request_order() {
    let t = test_app().await;
    seed(&t.pool).await;
    let request = send_json(Method::POST, "/api/channels/batch", json!({ "channelIds": ["CNOPE", "C2", "C1"] }));
    let (status, body) = call(&t.app, request).await;
    assert_eq!(status, StatusCode::OK);
    let ids: Vec<&str> = body.as_array().unwrap().iter().map(|c| c["channelId"].as_str().unwrap()).collect();
    assert_eq!(ids, ["C2", "C1"]);
    assert_eq!(body[0]["messages"], json!([]));

    // Up to 500 channels per request (unknown ids are simply skipped), 501 is refused.
    let many: Vec<String> = (0..500).map(|i| format!("C{i}")).collect();
    let (status, _) = call(&t.app, send_json(Method::POST, "/api/channels/batch", json!({ "channelIds": many }))).await;
    assert_eq!(status, StatusCode::OK);
    let too_many: Vec<String> = (0..501).map(|i| format!("C{i}")).collect();
    let (status, _) = call(&t.app, send_json(Method::POST, "/api/channels/batch", json!({ "channelIds": too_many }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn users_come_from_the_database_and_unknown_ones_ask_slack() {
    let t = test_app().await;
    seed(&t.pool).await;
    let (status, body) = call(&t.app, get("/api/users/U1", Some(API_KEY))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["userId"], "U1");
    assert_eq!(body["displayName"], "Alice");
    assert_eq!(body["imageUrl"], "https://avatars/u1.png");

    // Not stored -> users.info, which is offline in this test.
    let (status, _) = call(&t.app, get("/api/users/UNOPE", Some(API_KEY))).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn emoji_is_a_name_to_url_map() {
    let t = test_app().await;
    let emoji = HashMap::from([("party".to_string(), "https://e/party.png".to_string())]);
    store::replace_emoji(&t.pool, &emoji).await.unwrap();
    let (status, body) = call(&t.app, get("/api/emoji", Some(API_KEY))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "party": "https://e/party.png" }));
}

#[tokio::test]
async fn writes_validate_their_input() {
    let t = test_app().await;
    seed(&t.pool).await;

    let (status, _) = call(&t.app, send_json(Method::POST, "/api/messages", json!({ "channel": "times_alice", "text": "  " }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call(&t.app, send_json(Method::POST, "/api/messages", json!({ "channel": "nope", "text": "hi" }))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // The channel resolves, but Slack is offline.
    let (status, _) = call(&t.app, send_json(Method::POST, "/api/messages", json!({ "channel": "TIMES_ALICE", "text": "hi" }))).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);

    let (status, _) = call(
        &t.app,
        send_json(Method::POST, "/api/reactions/toggle", json!({ "channelId": "C1", "ts": "1.1", "name": "::" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_image_proxy_refuses_internal_addresses() {
    let t = test_app().await;
    for url in ["http://127.0.0.1:8787/api/health", "http://localhost/x", "file:///etc/passwd", "nonsense"] {
        let encoded: String = url.bytes().map(|b| format!("%{b:02X}")).collect();
        let (status, _) = call(&t.app, get(&format!("/api/images?url={encoded}"), Some(API_KEY))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{url}");
    }
}

#[tokio::test]
async fn sync_status_lists_targets_with_their_state() {
    let t = test_app().await;
    seed(&t.pool).await;
    store::reconcile_prefix_targets(&t.pool, "times", 5).await.unwrap();
    store::record_sync_success(&t.pool, "C1", Some("1700000002.000100"), 99).await.unwrap();
    store::record_sync_failure(&t.pool, "CGONE", "missing_scope").await.unwrap();

    let (status, body) = call(&t.app, get("/api/sync/status", Some(API_KEY))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["running"], false);
    assert_eq!(body["channelPrefix"], "times");
    assert!(body.get("lastCatalogAt").is_some());
    assert_eq!(
        body["channels"],
        json!([
            { "channelId": "C1", "source": "prefix", "lastMessageTs": "1700000002.000100",
              "lastSyncedAt": 99, "lastError": null },
            { "channelId": "CGONE", "source": null, "lastMessageTs": null,
              "lastSyncedAt": null, "lastError": "missing_scope" }
        ])
    );
}

#[tokio::test]
async fn custom_targets_can_be_read_and_replaced() {
    let t = test_app().await;
    seed(&t.pool).await;
    let (status, body) = call(&t.app, get("/api/sync/targets", Some(API_KEY))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "channelIds": [] }));

    // C2 has never been synced, so a background sync starts (and fails: Slack is offline).
    let request = send_json(Method::PUT, "/api/sync/targets", json!({ "channelIds": ["C2", " C2 ", "", "C1"] }));
    let (status, body) = call(&t.app, request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "channelIds": ["C2", "C1"] }));

    let (_, body) = call(&t.app, get("/api/sync/targets", Some(API_KEY))).await;
    let mut ids: Vec<String> = serde_json::from_value(body["channelIds"].clone()).unwrap();
    ids.sort();
    assert_eq!(ids, ["C1", "C2"]);

    let (_, body) = call(&t.app, send_json(Method::PUT, "/api/sync/targets", json!({ "channelIds": [] }))).await;
    assert_eq!(body, json!({ "channelIds": [] }));
}

#[tokio::test]
async fn unknown_channels_cannot_become_sync_targets() {
    let t = test_app().await;
    seed(&t.pool).await;
    let request = send_json(Method::PUT, "/api/sync/targets", json!({ "channelIds": ["C1", "CNOPE", "CALSO"] }));
    let (status, body) = call(&t.app, request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "unknown channel ids: [CNOPE, CALSO]");

    // Nothing was stored.
    let (_, body) = call(&t.app, get("/api/sync/targets", Some(API_KEY))).await;
    assert_eq!(body, json!({ "channelIds": [] }));
}

#[tokio::test]
async fn syncing_an_unknown_channel_is_a_404_and_a_known_one_is_accepted() {
    let t = test_app().await;
    seed(&t.pool).await;
    let post = |path: &str| {
        Request::builder()
            .method(Method::POST)
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {API_KEY}"))
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(call(&t.app, post("/api/sync/channels/CNOPE")).await.0, StatusCode::NOT_FOUND);
    let (status, body) = call(&t.app, post("/api/sync/channels/C1")).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body["started"], true);
    assert!(body["batchId"].as_str().is_some_and(|id| !id.is_empty()));
    let (status, body) = call(&t.app, post("/api/sync")).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(body["batchId"].is_string());
}

#[tokio::test]
async fn several_channels_sync_as_one_batch_and_bad_requests_are_refused() {
    use slacksitter_core::sync::SyncEvent;

    let t = test_app().await;
    seed(&t.pool).await;
    let mut events = t.service.subscribe();

    let request = send_json(Method::POST, "/api/sync/channels", json!({ "channelIds": ["C1", "C2", " C1 "] }));
    let (status, body) = call(&t.app, request).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let batch_id = body["batchId"].as_str().unwrap().to_string();

    // One batch: started(2) .. progress .. completed, all with the id the response gave.
    // (Slack is offline in this test, so both channels fail, which the batch counts.)
    let mut seen = Vec::new();
    while !matches!(seen.last(), Some(SyncEvent::Completed { .. })) {
        let event = tokio::time::timeout(std::time::Duration::from_secs(10), events.recv())
            .await
            .expect("the batch completes")
            .unwrap();
        if matches!(event, SyncEvent::Started { .. } | SyncEvent::Progress { .. } | SyncEvent::Completed { .. }) {
            seen.push(event);
        }
    }
    assert!(matches!(&seen[0], SyncEvent::Started { batch_id: id, total: 2, .. } if *id == batch_id));
    assert!(matches!(seen.last(), Some(SyncEvent::Completed { batch_id: id, failed: 2, .. }) if *id == batch_id));

    for (body, expected) in [
        (json!({ "channelIds": [] }), "channelIds must not be empty"),
        (json!({ "channelIds": ["C1", "CNOPE"] }), "unknown channel ids: [CNOPE]"),
    ] {
        let (status, body) = call(&t.app, send_json(Method::POST, "/api/sync/channels", body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], expected);
    }
    let too_many: Vec<String> = (0..501).map(|i| format!("C{i}")).collect();
    let (status, _) = call(&t.app, send_json(Method::POST, "/api/sync/channels", json!({ "channelIds": too_many }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn events_is_a_server_sent_event_stream() {
    let t = test_app().await;
    let response = t.app.clone().oneshot(get("/api/events", Some(API_KEY))).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "text/event-stream");

    // The first frame is the `ready` event.
    let mut body = response.into_body();
    let frame = body.frame().await.unwrap().unwrap();
    let text = String::from_utf8(frame.into_data().unwrap().to_vec()).unwrap();
    assert!(text.contains("event: ready"), "{text}");
}

#[tokio::test]
async fn sync_events_reach_subscribers_with_their_wire_names() {
    use slacksitter_core::sync::{SyncEvent, SyncKind};

    let pool = db::connect_memory().await.unwrap();
    let service = SyncService::new(pool, Arc::new(NoSlack), SyncConfig::default());
    let scheduler = Arc::new(SyncScheduler::new(service.clone()));
    let state = AppState::new(
        Secret::new(API_KEY),
        scheduler,
        Me {
            auth: AuthResult { workspace_url: String::new(), user_id: "UME".into(), current_user_avatar_uri: None },
        },
        ImageProxy::new(Secret::new("t")),
        std::env::temp_dir(),
    );
    let app = router(state, build_cors(&[]).unwrap());
    let response = app.oneshot(get("/api/events", Some(API_KEY))).await.unwrap();
    let mut body = response.into_body();
    let _ready = body.frame().await.unwrap().unwrap();

    service.emit(SyncEvent::Started { batch_id: "b1".into(), kind: SyncKind::Manual, total: 2 });
    service.emit(SyncEvent::Progress { batch_id: "b1".into(), kind: SyncKind::Manual, done: 1, total: 2 });
    service.emit(SyncEvent::ChannelUpdated { channel_id: "C1".into() });
    service.emit(SyncEvent::Error { message: "missing_scope".into() });
    service.emit(SyncEvent::Completed { batch_id: "b1".into(), kind: SyncKind::Scheduler, failed: 1 });

    let mut text = String::new();
    while !text.contains("sync-completed") {
        let frame = body.frame().await.unwrap().unwrap();
        text.push_str(&String::from_utf8(frame.into_data().unwrap().to_vec()).unwrap());
    }
    assert!(
        text.contains("event: sync-started\ndata: {\"batchId\":\"b1\",\"kind\":\"manual\",\"total\":2}"),
        "{text}"
    );
    assert!(
        text.contains("event: sync-progress\ndata: {\"batchId\":\"b1\",\"done\":1,\"kind\":\"manual\",\"total\":2}"),
        "{text}"
    );
    assert!(text.contains("event: channel-updated\ndata: {\"channelId\":\"C1\"}"), "{text}");
    assert!(text.contains("event: sync-error\ndata: {\"message\":\"missing_scope\"}"), "{text}");
    assert!(
        text.contains("event: sync-completed\ndata: {\"batchId\":\"b1\",\"failed\":1,\"kind\":\"scheduler\"}"),
        "{text}"
    );
}

#[tokio::test]
async fn cors_allows_only_the_configured_origins() {
    let t = test_app().await;
    let preflight = |origin: &str| {
        Request::builder()
            .method(Method::OPTIONS)
            .uri("/api/channels")
            .header(header::ORIGIN, origin)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "authorization")
            .body(Body::empty())
            .unwrap()
    };
    // Preflight carries no credentials, so it must not need the API key.
    let allowed = t.app.clone().oneshot(preflight("http://localhost:1420")).await.unwrap();
    assert!(allowed.status().is_success());
    assert_eq!(allowed.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN], "http://localhost:1420");

    let denied = t.app.clone().oneshot(preflight("http://evil.example")).await.unwrap();
    assert!(denied.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
}

#[tokio::test]
async fn logs_are_empty_when_nothing_has_been_written() {
    let t = test_app().await;
    let response = t.app.clone().oneshot(get("/api/logs?lines=50", Some(API_KEY))).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers()[header::CONTENT_TYPE].to_str().unwrap().starts_with("text/plain"));
}
