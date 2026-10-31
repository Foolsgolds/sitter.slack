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
use super::dto::{SlackConversation, SlackMessage, SlackUser};

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

    /// `conversations.history`, limit 10 by default, optional `oldest` for
    /// incremental refresh. Returned ordered descending by ts (newest first).
    pub async fn conversation_history(
        &self,
        channel_id: &str,
        oldest: Option<&str>,
        limit: u32,
    ) -> AppResult<Vec<SlackMessage>> {
        let limit_str = limit.to_string();
        let mut query: Vec<(&str, &str)> = vec![("channel", channel_id), ("limit", &limit_str)];
        if let Some(oldest) = oldest {
            query.push(("oldest", oldest));
        }
        let body = self
            .with_retry(|| self.get("conversations.history", &query))
            .await?;

        let mut messages: Vec<SlackMessage> =
            serde_json::from_value(body.get("messages").cloned().unwrap_or(json!([])))
                .map_err(|e| AppError::Slack(format!("failed to parse messages: {e}")))?;

        messages.sort_by(|a, b| parse_ts(&b.ts).partial_cmp(&parse_ts(&a.ts)).unwrap());
        Ok(messages)
    }

    /// `conversations.replies`, limit 20, parent message excluded, ordered
    /// ascending by ts. Only meant to be called for messages with reply_count > 0.
    pub async fn conversation_replies(
        &self,
        channel_id: &str,
        thread_ts: &str,
    ) -> AppResult<Vec<SlackMessage>> {
        let query = [("channel", channel_id), ("ts", thread_ts), ("limit", "20")];
        let body = self
            .with_retry(|| self.get("conversations.replies", &query))
            .await?;

        let mut replies: Vec<SlackMessage> =
            serde_json::from_value(body.get("messages").cloned().unwrap_or(json!([])))
                .map_err(|e| AppError::Slack(format!("failed to parse replies: {e}")))?;

        replies.retain(|m| m.ts.as_deref() != Some(thread_ts));
        replies.sort_by(|a, b| parse_ts(&a.ts).partial_cmp(&parse_ts(&b.ts)).unwrap());
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

    /// Fetches the exact message by timestamp and checks whether `user_id`
    /// has already reacted with `reaction_name`. Used to decide add vs.
    /// remove before toggling (not a blind toggle call).
    pub async fn has_user_reaction(
        &self,
        channel_id: &str,
        ts: &str,
        reaction_name: &str,
        user_id: &str,
    ) -> AppResult<bool> {
        let query = [
            ("channel", channel_id),
            ("oldest", ts),
            ("latest", ts),
            ("inclusive", "true"),
            ("limit", "1"),
        ];
        let body = self.with_retry(|| self.get("conversations.history", &query)).await?;

        let messages = body.get("messages").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let Some(message) = messages.first() else {
            return Ok(false);
        };
        let has_reacted = message
            .get("reactions")
            .and_then(|v| v.as_array())
            .map(|reactions| {
                reactions.iter().any(|reaction| {
                    let name_matches = reaction.get("name").and_then(|v| v.as_str()) == Some(reaction_name);
                    let user_matches = reaction
                        .get("users")
                        .and_then(|v| v.as_array())
                        .map(|users| users.iter().any(|u| u.as_str() == Some(user_id)))
                        .unwrap_or(false);
                    name_matches && user_matches
                })
            })
            .unwrap_or(false);
        Ok(has_reacted)
    }
}

fn parse_ts(ts: &Option<String>) -> f64 {
    ts.as_deref().and_then(|s| s.parse::<f64>().ok()).unwrap_or(f64::MIN)
}
