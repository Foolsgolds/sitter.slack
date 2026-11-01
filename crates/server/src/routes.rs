//! The REST + SSE surface. JSON keys are camelCase and the models are the
//! same ones the Tauri commands used to return.

use std::collections::HashMap;
use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use slacksitter_core::db::{store, view};
use slacksitter_core::slack::api::SlackApi;
use slacksitter_core::slack::models::{ChannelSummary, ChannelWithMessages, SlackUserProfileInfo};
use slacksitter_core::sync::SyncEvent;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::auth;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

const DEFAULT_MESSAGE_LIMIT: u32 = 10;
const MAX_MESSAGE_LIMIT: u32 = 100;
const MAX_BATCH_CHANNELS: usize = 200;
const DEFAULT_LOG_LINES: usize = 500;
const MAX_LOG_LINES: usize = 5000;

/// The whole app. `cors` sits outside the API-key check so preflight requests
/// (which carry no credentials) are answered before authentication.
pub fn router<A: SlackApi + 'static>(state: AppState<A>, cors: CorsLayer) -> Router {
    let protected = Router::new()
        .route("/api/me", get(me::<A>))
        .route("/api/channels", get(channels::<A>))
        .route("/api/channels/batch", post(channels_batch::<A>))
        .route("/api/channels/{id}", get(channel::<A>))
        .route("/api/users/{id}", get(user::<A>))
        .route("/api/emoji", get(emoji::<A>))
        .route("/api/messages", post(post_message::<A>))
        .route("/api/reactions/toggle", post(toggle_reaction::<A>))
        .route("/api/images", get(image::<A>))
        .route("/api/sync/status", get(sync_status::<A>))
        .route("/api/sync", post(sync_all::<A>))
        .route("/api/sync/channels/{id}", post(sync_channel::<A>))
        .route("/api/sync/targets", get(get_targets::<A>).put(put_targets::<A>))
        .route("/api/events", get(events::<A>))
        .route("/api/logs", get(logs::<A>))
        .with_state(state.clone())
        .layer(middleware::from_fn_with_state(state.api_key().clone(), auth::require_api_key));

    Router::new()
        .route("/api/health", get(health))
        .merge(protected)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
}

#[derive(Deserialize)]
struct LimitQuery {
    limit: Option<u32>,
}

fn message_limit(query: &LimitQuery) -> u32 {
    query.limit.unwrap_or(DEFAULT_MESSAGE_LIMIT).clamp(1, MAX_MESSAGE_LIMIT)
}

// ------------------------------------------------------------------ identity

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MeResponse<'a> {
    #[serde(flatten)]
    auth: &'a slacksitter_core::slack::models::AuthResult,
    /// Boards show the channels whose name starts with this.
    channel_prefix: &'a str,
}

async fn me<A: SlackApi + 'static>(State(state): State<AppState<A>>) -> Response {
    Json(MeResponse {
        auth: &state.me().auth,
        channel_prefix: state.channel_prefix(),
    })
    .into_response()
}

// ------------------------------------------------------------------ channels

async fn channels<A: SlackApi + 'static>(State(state): State<AppState<A>>) -> ApiResult<Json<Vec<ChannelSummary>>> {
    Ok(Json(store::list_catalog(state.pool()).await?))
}

async fn channel<A: SlackApi + 'static>(
    State(state): State<AppState<A>>,
    Path(id): Path<String>,
    Query(query): Query<LimitQuery>,
) -> ApiResult<Json<ChannelWithMessages>> {
    let workspace_url = workspace_url(&state);
    view::channel_with_messages(state.pool(), &id, message_limit(&query), workspace_url)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("channel not found: {id}")))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchRequest {
    channel_ids: Vec<String>,
}

async fn channels_batch<A: SlackApi + 'static>(
    State(state): State<AppState<A>>,
    Query(query): Query<LimitQuery>,
    Json(body): Json<BatchRequest>,
) -> ApiResult<Json<Vec<ChannelWithMessages>>> {
    if body.channel_ids.len() > MAX_BATCH_CHANNELS {
        return Err(ApiError::bad_request(format!("at most {MAX_BATCH_CHANNELS} channels per request")));
    }
    let channels =
        view::channels_with_messages(state.pool(), &body.channel_ids, message_limit(&query), workspace_url(&state))
            .await?;
    Ok(Json(channels))
}

fn workspace_url<A: SlackApi>(state: &AppState<A>) -> Option<&str> {
    Some(state.me().auth.workspace_url.as_str()).filter(|url| !url.is_empty())
}

// ----------------------------------------------------------- users and emoji

async fn user<A: SlackApi + 'static>(
    State(state): State<AppState<A>>,
    Path(id): Path<String>,
) -> ApiResult<Json<SlackUserProfileInfo>> {
    if let Some(profile) = view::user_profile(state.pool(), &id).await? {
        return Ok(Json(profile));
    }
    state.service().fetch_user(&id).await?;
    view::user_profile(state.pool(), &id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("user not found: {id}")))
}

async fn emoji<A: SlackApi + 'static>(
    State(state): State<AppState<A>>,
) -> ApiResult<Json<HashMap<String, String>>> {
    Ok(Json(view::emoji_map(state.pool()).await?))
}

// ------------------------------------------------------------- writing to Slack

#[derive(Deserialize)]
struct PostMessageRequest {
    channel: String,
    text: String,
}

async fn post_message<A: SlackApi + 'static>(
    State(state): State<AppState<A>>,
    Json(body): Json<PostMessageRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let channel_id = state.service().post_message(&body.channel, &body.text).await?;
    tracing::info!(channel = %body.channel, "message posted");

    // Pull the new message into the database right away, without making the
    // caller wait on Slack's rate limit.
    let service = state.service().clone();
    let sync_id = channel_id.clone();
    tokio::spawn(async move {
        let _ = service.sync_channel(&sync_id).await;
    });
    Ok(Json(json!({ "ok": true, "channelId": channel_id })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToggleReactionRequest {
    channel_id: String,
    ts: String,
    name: String,
}

async fn toggle_reaction<A: SlackApi + 'static>(
    State(state): State<AppState<A>>,
    Json(body): Json<ToggleReactionRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let name = body.name.trim().trim_matches(':');
    if name.is_empty() {
        return Err(ApiError::bad_request("reaction name must not be empty"));
    }
    let reacted = state
        .service()
        .toggle_reaction(&body.channel_id, &body.ts, name, &state.me().auth.user_id)
        .await?;
    tracing::info!(channel_id = %body.channel_id, reaction = %name, reacted, "reaction toggled");
    Ok(Json(json!({ "reacted": reacted })))
}

// -------------------------------------------------------------------- images

#[derive(Deserialize)]
struct ImageQuery {
    url: String,
}

async fn image<A: SlackApi + 'static>(
    State(state): State<AppState<A>>,
    Query(query): Query<ImageQuery>,
) -> ApiResult<Response> {
    let fetched = state.images().fetch(&query.url).await?;
    let content_type = HeaderValue::from_str(&fetched.content_type)
        .map_err(|_| ApiError::new(StatusCode::BAD_GATEWAY, "invalid content type"))?;
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=3600")),
        ],
        fetched.bytes,
    )
        .into_response())
}

// ---------------------------------------------------------------------- sync

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChannelSyncStatus {
    channel_id: String,
    /// `prefix`, `custom`, or absent when the channel is no longer a target.
    source: Option<String>,
    last_message_ts: Option<String>,
    last_synced_at: Option<i64>,
    last_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncStatusResponse {
    running: bool,
    last_started_at: Option<i64>,
    last_finished_at: Option<i64>,
    last_catalog_at: Option<i64>,
    channel_prefix: String,
    channels: Vec<ChannelSyncStatus>,
}

async fn sync_status<A: SlackApi + 'static>(State(state): State<AppState<A>>) -> ApiResult<Json<SyncStatusResponse>> {
    let status = state.scheduler().status();
    let targets = store::list_targets(state.pool()).await?;
    let mut states: HashMap<String, store::SyncState> = store::list_sync_states(state.pool())
        .await?
        .into_iter()
        .map(|s| (s.channel_id.clone(), s))
        .collect();

    let mut channels: Vec<ChannelSyncStatus> = targets
        .into_iter()
        .map(|target| {
            let sync = states.remove(&target.channel_id).unwrap_or_default();
            ChannelSyncStatus {
                channel_id: target.channel_id,
                source: Some(target.source),
                last_message_ts: sync.last_message_ts,
                last_synced_at: sync.last_synced_at,
                last_error: sync.last_error,
            }
        })
        .collect();
    let mut leftovers: Vec<_> = states.into_values().collect();
    leftovers.sort_by(|a, b| a.channel_id.cmp(&b.channel_id));
    channels.extend(leftovers.into_iter().map(|sync| ChannelSyncStatus {
        channel_id: sync.channel_id,
        source: None,
        last_message_ts: sync.last_message_ts,
        last_synced_at: sync.last_synced_at,
        last_error: sync.last_error,
    }));

    Ok(Json(SyncStatusResponse {
        running: status.running,
        last_started_at: status.last_started_at,
        last_finished_at: status.last_finished_at,
        last_catalog_at: status.last_catalog_at,
        channel_prefix: state.channel_prefix().to_string(),
        channels,
    }))
}

/// Starts a pass over every target now (not spaced out) and returns at once;
/// progress arrives on `/api/events`. 409 while a pass is already running.
async fn sync_all<A: SlackApi + 'static>(State(state): State<AppState<A>>) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    state.scheduler().spawn_cycle(false)?;
    Ok((StatusCode::ACCEPTED, Json(json!({ "started": true }))))
}

async fn sync_channel<A: SlackApi + 'static>(
    State(state): State<AppState<A>>,
    Path(id): Path<String>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    if !store::channel_exists(state.pool(), &id).await? {
        return Err(ApiError::not_found(format!("channel not found: {id}")));
    }
    spawn_channel_sync(&state, vec![id]);
    Ok((StatusCode::ACCEPTED, Json(json!({ "started": true }))))
}

/// Syncs the channels one after another in the background, announcing the
/// batch with `sync-started` / `sync-completed` like a scheduler pass.
fn spawn_channel_sync<A: SlackApi + 'static>(state: &AppState<A>, channel_ids: Vec<String>) {
    let service = state.service().clone();
    tokio::spawn(async move {
        service.emit(SyncEvent::Started {
            total: channel_ids.len(),
        });
        let mut failed = 0;
        for (index, id) in channel_ids.iter().enumerate() {
            if service.sync_channel(id).await.is_err() {
                failed += 1;
            }
            service.emit(SyncEvent::Progress {
                done: index + 1,
                total: channel_ids.len(),
            });
        }
        service.emit(SyncEvent::Completed { failed });
    });
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TargetsBody {
    channel_ids: Vec<String>,
}

async fn get_targets<A: SlackApi + 'static>(State(state): State<AppState<A>>) -> ApiResult<Json<TargetsBody>> {
    let channel_ids = store::list_targets(state.pool())
        .await?
        .into_iter()
        .filter(|t| t.source == store::SOURCE_CUSTOM)
        .map(|t| t.channel_id)
        .collect();
    Ok(Json(TargetsBody { channel_ids }))
}

/// Replaces the custom targets. Channels that have never been synced start
/// syncing in the background so a new board fills without waiting for the next pass.
async fn put_targets<A: SlackApi + 'static>(
    State(state): State<AppState<A>>,
    Json(body): Json<TargetsBody>,
) -> ApiResult<Json<TargetsBody>> {
    let mut channel_ids: Vec<String> = Vec::new();
    for id in body.channel_ids {
        let id = id.trim().to_string();
        if !id.is_empty() && !channel_ids.contains(&id) {
            channel_ids.push(id);
        }
    }
    state.service().set_custom_targets(&channel_ids).await?;

    let mut fresh = Vec::new();
    for id in &channel_ids {
        let known = store::channel_exists(state.pool(), id).await?;
        let synced = store::get_sync_state(state.pool(), id).await?.last_synced_at.is_some();
        if known && !synced {
            fresh.push(id.clone());
        }
    }
    if !fresh.is_empty() {
        spawn_channel_sync(&state, fresh);
    }
    Ok(Json(TargetsBody { channel_ids }))
}

// -------------------------------------------------------------------- events

fn sse_event(event: SyncEvent) -> Event {
    let (name, data) = match event {
        SyncEvent::Started { total } => ("sync-started", json!({ "total": total })),
        SyncEvent::Progress { done, total } => ("sync-progress", json!({ "done": done, "total": total })),
        SyncEvent::ChannelUpdated { channel_id } => ("channel-updated", json!({ "channelId": channel_id })),
        SyncEvent::Completed { failed } => ("sync-completed", json!({ "failed": failed })),
        SyncEvent::Error { message } => ("sync-error", json!({ "message": message })),
    };
    Event::default().event(name).data(data.to_string())
}

/// Server-sent events. The first event is `ready`, so a client knows the
/// stream is live; after that: `sync-started`, `sync-progress`,
/// `channel-updated`, `sync-completed`, `sync-error`.
async fn events<A: SlackApi + 'static>(
    State(state): State<AppState<A>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let receiver = state.service().subscribe();
    let ready = tokio_stream::once(Ok(Event::default().event("ready").data("{}")));
    // A lagging subscriber just misses events; the client's polling fallback catches up.
    let updates = BroadcastStream::new(receiver).filter_map(|item| item.ok().map(|event| Ok(sse_event(event))));
    Sse::new(ready.chain(updates)).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

// ---------------------------------------------------------------------- logs

#[derive(Deserialize)]
struct LogsQuery {
    lines: Option<usize>,
}

async fn logs<A: SlackApi + 'static>(
    State(state): State<AppState<A>>,
    Query(query): Query<LogsQuery>,
) -> ApiResult<Response> {
    let lines = query.lines.unwrap_or(DEFAULT_LOG_LINES).clamp(1, MAX_LOG_LINES);
    let dir = state.log_dir().clone();
    let text = tokio::task::spawn_blocking(move || crate::logging::read_recent(&dir, lines))
        .await
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "log reader failed"))?
        .map_err(|err| {
            tracing::error!("cannot read the log directory: {err}");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "cannot read the logs")
        })?;
    Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], text).into_response())
}
