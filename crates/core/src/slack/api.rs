//! The slice of the Slack Web API that the sync worker depends on, behind a
//! trait so tests can replace Slack with an in-memory fake.

use std::collections::HashMap;
use std::future::Future;

use serde_json::Value;

use crate::error::AppResult;

use super::client::SlackClient;
use super::dto::{SlackConversation, SlackUser};

/// One page of `conversations.history`, newest message first. Messages stay
/// raw JSON so they can be stored verbatim.
#[derive(Debug, Clone, Default)]
pub struct HistoryPage {
    pub messages: Vec<Value>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

pub trait SlackApi: Send + Sync {
    /// One page of `conversations.list` (non-archived public + private channels).
    fn list_channels_page(
        &self,
        cursor: Option<&str>,
    ) -> impl Future<Output = AppResult<(Vec<SlackConversation>, Option<String>)>> + Send;

    /// `emoji.list`, raw (alias values not yet resolved).
    fn emoji_list(&self) -> impl Future<Output = AppResult<HashMap<String, String>>> + Send;

    /// One page of `conversations.history`, newest first; `cursor` continues an earlier page.
    fn history_page(
        &self,
        channel_id: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> impl Future<Output = AppResult<HistoryPage>> + Send;

    /// `conversations.replies` without the parent, oldest reply first.
    fn replies(
        &self,
        channel_id: &str,
        thread_ts: &str,
    ) -> impl Future<Output = AppResult<Vec<Value>>> + Send;

    fn user_info(&self, user_id: &str) -> impl Future<Output = AppResult<SlackUser>> + Send;

    /// `chat.postMessage`.
    fn post_message(&self, channel_id: &str, text: &str) -> impl Future<Output = AppResult<()>> + Send;

    /// `reactions.add` (`already_reacted` counts as success).
    fn add_reaction(&self, channel_id: &str, ts: &str, name: &str) -> impl Future<Output = AppResult<()>> + Send;

    /// `reactions.remove` (`no_reaction` counts as success).
    fn remove_reaction(&self, channel_id: &str, ts: &str, name: &str) -> impl Future<Output = AppResult<()>> + Send;

    /// The message with exactly this `ts`, raw; `None` when it no longer exists.
    fn message_at(&self, channel_id: &str, ts: &str) -> impl Future<Output = AppResult<Option<Value>>> + Send;
}

impl SlackApi for SlackClient {
    async fn list_channels_page(
        &self,
        cursor: Option<&str>,
    ) -> AppResult<(Vec<SlackConversation>, Option<String>)> {
        SlackClient::list_channels_page(self, cursor).await
    }

    async fn emoji_list(&self) -> AppResult<HashMap<String, String>> {
        SlackClient::emoji_list(self).await
    }

    async fn history_page(
        &self,
        channel_id: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> AppResult<HistoryPage> {
        SlackClient::conversation_history_page(self, channel_id, cursor, limit).await
    }

    async fn replies(&self, channel_id: &str, thread_ts: &str) -> AppResult<Vec<Value>> {
        SlackClient::conversation_replies_raw(self, channel_id, thread_ts).await
    }

    async fn user_info(&self, user_id: &str) -> AppResult<SlackUser> {
        SlackClient::user_info(self, user_id).await
    }

    async fn post_message(&self, channel_id: &str, text: &str) -> AppResult<()> {
        SlackClient::post_message(self, channel_id, text).await
    }

    async fn add_reaction(&self, channel_id: &str, ts: &str, name: &str) -> AppResult<()> {
        SlackClient::add_reaction(self, channel_id, ts, name).await
    }

    async fn remove_reaction(&self, channel_id: &str, ts: &str, name: &str) -> AppResult<()> {
        SlackClient::remove_reaction(self, channel_id, ts, name).await
    }

    async fn message_at(&self, channel_id: &str, ts: &str) -> AppResult<Option<Value>> {
        SlackClient::conversation_message_raw(self, channel_id, ts).await
    }
}
