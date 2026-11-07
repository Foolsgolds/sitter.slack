//! Slack Web API endpoint wrappers. Port of the call sites in the legacy
//! `SlackService.cs` (both the SlackNet-backed calls and the raw-HttpClient
//! calls) onto plain `reqwest` requests through `SlackClient`.
//!
//! Retry policy: every call here goes through `with_retry`. The legacy app left
//! `Conversations.List` out of its retry wrapper, but a rate-limited page then
//! failed the whole workspace refresh (seen in practice on large workspaces),
//! so channel listing is retried too.

use std::collections::HashMap;

use serde_json::json;

use crate::error::{AppError, AppResult};

use super::client::SlackClient;
use super::api::HistoryPage;
use super::dto::{SlackConversation, SlackUser};

impl SlackClient {
    /// `auth.test` — returns (workspace_url, user_id).
    pub async fn auth_test(&self) -> AppResult<(String, String)> {
        let body = self
            .with_retry(|| self.get("auth.test", &[]))
            .await?;
        let url = body
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim_end_matches('/')
            .to_string();
        let user_id = body
            .get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if user_id.is_empty() {
            return Err(AppError::Slack("auth.test returned no user_id".to_string()));
        }
        Ok((url, user_id))
    }

    /// One page of `conversations.list` (public+private, excluding archived).
    /// 200 per page keeps the page count (and so the tier-2 rate-limit
    /// pressure) low on workspaces with many channels.
    pub async fn list_channels_page(
        &self,
        cursor: Option<&str>,
    ) -> AppResult<(Vec<SlackConversation>, Option<String>)> {
        let query = [
            ("types", "public_channel,private_channel"),
            ("exclude_archived", "true"),
            ("limit", "200"),
            ("cursor", cursor.unwrap_or("")),
        ];
        let body = self.with_retry(|| self.get("conversations.list", &query)).await?;

        let channels: Vec<SlackConversation> = serde_json::from_value(
            body.get("channels").cloned().unwrap_or(json!([])),
        )
        .map_err(|e| AppError::Slack(format!("failed to parse channels: {e}")))?;

        let next_cursor = body
            .get("response_metadata")
            .and_then(|m| m.get("next_cursor"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        Ok((channels, next_cursor))
    }

    /// One page of `conversations.history` with the messages left as raw JSON
    /// (for the sync worker, which stores them verbatim).
    /// Messages are ordered newest first; `cursor` continues an earlier page.
    pub async fn conversation_history_page(
        &self,
        channel_id: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> AppResult<HistoryPage> {
        let limit_str = limit.to_string();
        let mut query: Vec<(&str, &str)> = vec![("channel", channel_id), ("limit", &limit_str)];
        if let Some(cursor) = cursor {
            query.push(("cursor", cursor));
        }
        let body = self
            .with_retry(|| self.get("conversations.history", &query))
            .await?;

        let mut messages = body
            .get("messages")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        messages.sort_by(|a, b| raw_ts(b).partial_cmp(&raw_ts(a)).unwrap());

        Ok(HistoryPage {
            messages,
            has_more: body.get("has_more").and_then(|v| v.as_bool()).unwrap_or(false),
            next_cursor: body
                .get("response_metadata")
                .and_then(|m| m.get("next_cursor"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()),
        })
    }

    /// Raw-JSON counterpart of `conversation_replies` (same limit 20, parent
    /// excluded, oldest first).
    pub async fn conversation_replies_raw(
        &self,
        channel_id: &str,
        thread_ts: &str,
    ) -> AppResult<Vec<serde_json::Value>> {
        let query = [("channel", channel_id), ("ts", thread_ts), ("limit", "20")];
        let body = self
            .with_retry(|| self.get("conversations.replies", &query))
            .await?;

        let mut replies = body
            .get("messages")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        replies.retain(|m| m.get("ts").and_then(|v| v.as_str()) != Some(thread_ts));
        replies.sort_by(|a, b| raw_ts(a).partial_cmp(&raw_ts(b)).unwrap());
        Ok(replies)
    }

    /// `users.info` for a single user.
    pub async fn user_info(&self, user_id: &str) -> AppResult<SlackUser> {
        let query = [("user", user_id)];
        let body = self.with_retry(|| self.get("users.info", &query)).await?;
        serde_json::from_value(body.get("user").cloned().unwrap_or(json!({})))
            .map_err(|e| AppError::Slack(format!("failed to parse user: {e}")))
    }

    /// `emoji.list` — raw name -> value map (value may be `alias:<name>`).
    pub async fn emoji_list(&self) -> AppResult<HashMap<String, String>> {
        let body = self.with_retry(|| self.get("emoji.list", &[])).await?;
        serde_json::from_value(body.get("emoji").cloned().unwrap_or(json!({})))
            .map_err(|e| AppError::Slack(format!("failed to parse emoji map: {e}")))
    }

    /// `chat.postMessage`.
    pub async fn post_message(&self, channel_id: &str, text: &str) -> AppResult<()> {
        let body = json!({ "channel": channel_id, "text": text });
        self.with_retry(|| self.post_json("chat.postMessage", &body)).await?;
        Ok(())
    }

    /// `reactions.add`, treating `already_reacted` as success (idempotent).
    pub async fn add_reaction(&self, channel_id: &str, ts: &str, name: &str) -> AppResult<()> {
        self.toggle_reaction_call("reactions.add", channel_id, ts, name, "already_reacted")
            .await
    }

    /// `reactions.remove`, treating `no_reaction` as success (idempotent).
    pub async fn remove_reaction(&self, channel_id: &str, ts: &str, name: &str) -> AppResult<()> {
        self.toggle_reaction_call("reactions.remove", channel_id, ts, name, "no_reaction")
            .await
    }

    async fn toggle_reaction_call(
        &self,
        method: &str,
        channel_id: &str,
        ts: &str,
        name: &str,
        accepted_error: &str,
    ) -> AppResult<()> {
        let body = json!({ "channel": channel_id, "timestamp": ts, "name": name });
        let result = self.with_retry(|| self.post_json(method, &body)).await;

        match result {
            Ok(_) => Ok(()),
            Err(AppError::Slack(error)) if error == accepted_error => Ok(()),
            Err(other) => Err(other),
        }
    }

}

fn raw_ts(message: &serde_json::Value) -> f64 {
    message
        .get("ts")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(f64::MIN)
}

impl SlackClient {
    /// The single message with exactly this `ts` as raw JSON (`conversations.history`
    /// with `oldest` = `latest` = `ts`, inclusive), or `None` when it is gone.
    pub async fn conversation_message_raw(
        &self,
        channel_id: &str,
        ts: &str,
    ) -> AppResult<Option<serde_json::Value>> {
        let query = [
            ("channel", channel_id),
            ("oldest", ts),
            ("latest", ts),
            ("inclusive", "true"),
            ("limit", "1"),
        ];
        let body = self.with_retry(|| self.get("conversations.history", &query)).await?;
        Ok(body
            .get("messages")
            .and_then(|v| v.as_array())
            .and_then(|messages| messages.first().cloned()))
    }
}
