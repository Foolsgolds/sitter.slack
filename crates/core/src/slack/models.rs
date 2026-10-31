use serde::Serialize;

use super::dto::SlackMessage;
use super::extract::{extract_images, extract_reactions};
use super::parser::parse_segments;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SegmentType {
    Text,
    Link,
    Emoji,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageInlineSegment {
    pub seg_type: SegmentType,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(default)]
    pub is_bold: bool,
    #[serde(default)]
    pub is_italic: bool,
    #[serde(default)]
    pub is_strikethrough: bool,
    #[serde(default)]
    pub is_code: bool,
}

impl MessageInlineSegment {
    pub fn text(text: impl Into<String>, bold: bool, italic: bool, strike: bool, code: bool) -> Self {
        Self {
            seg_type: SegmentType::Text,
            text: text.into(),
            uri: None,
            is_bold: bold,
            is_italic: italic,
            is_strikethrough: strike,
            is_code: code,
        }
    }

    pub fn emoji(name: impl Into<String>) -> Self {
        Self {
            seg_type: SegmentType::Emoji,
            text: name.into(),
            uri: None,
            is_bold: false,
            is_italic: false,
            is_strikethrough: false,
            is_code: false,
        }
    }

    pub fn link(display_text: impl Into<String>, uri: impl Into<String>) -> Self {
        Self {
            seg_type: SegmentType::Link,
            text: display_text.into(),
            uri: Some(uri.into()),
            is_bold: false,
            is_italic: false,
            is_strikethrough: false,
            is_code: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MessageImageItem {
    pub candidate_urls: Vec<String>,
}

impl MessageImageItem {
    /// Filters blank candidates and de-dupes case-insensitively, preserving
    /// first-seen order (mirrors the legacy MessageImageItem constructor).
    pub fn new(candidates: Vec<Option<&str>>) -> Self {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for candidate in candidates.into_iter().flatten() {
            let trimmed = candidate.trim();
            if trimmed.is_empty() {
                continue;
            }
            let key = trimmed.to_lowercase();
            if seen.insert(key) {
                result.push(trimmed.to_string());
            }
        }
        Self { candidate_urls: result }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageReactionItem {
    pub name: String,
    pub count: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageDisplayItem {
    pub user: Option<String>,
    pub ts: Option<String>,
    pub text: Option<String>,
    pub user_avatar_uri: Option<String>,
    pub permalink_uri: Option<String>,
    pub segments: Vec<MessageInlineSegment>,
    pub images: Vec<MessageImageItem>,
    pub reactions: Vec<MessageReactionItem>,
    pub replies: Vec<MessageDisplayItem>,
}

/// Builds a display item from a raw Slack message, mirroring
/// `MessageDisplayItem`'s constructor in the legacy WinUI app.
pub fn build_message_display_item(
    message: &SlackMessage,
    channel_id: &str,
    workspace_url: Option<&str>,
    user_avatar_url: Option<&str>,
    replies: Vec<MessageDisplayItem>,
) -> MessageDisplayItem {
    MessageDisplayItem {
        user: message.user.clone(),
        ts: message.ts.clone(),
        text: message.text.clone(),
        user_avatar_uri: user_avatar_url.map(|s| s.to_string()),
        permalink_uri: build_permalink_uri(workspace_url, Some(channel_id), message.ts.as_deref()),
        segments: parse_segments(message.text.as_deref()),
        images: extract_images(message),
        reactions: extract_reactions(message),
        replies,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelWithMessages {
    pub channel_id: String,
    pub channel_uri: Option<String>,
    pub name: String,
    pub topic_value: Option<String>,
    pub purpose_value: Option<String>,
    pub num_members: Option<i64>,
    pub is_member: bool,
    pub last_message_ts: Option<String>,
    pub last_fetched_ts: Option<String>,
    pub messages: Vec<MessageDisplayItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSummary {
    pub channel_id: String,
    pub name: String,
    pub is_member: bool,
    pub is_archived: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SlackUserProfileInfo {
    pub user_id: String,
    pub user_name: String,
    pub display_name: String,
    pub real_name: String,
    pub title: String,
    pub status_text: String,
    pub image_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResult {
    pub workspace_url: String,
    pub user_id: String,
    pub current_user_avatar_uri: Option<String>,
}

/// `{workspaceUrl}/archives/{channelId}` — pure string construction, no API call.
pub fn build_channel_uri(workspace_url: Option<&str>, channel_id: Option<&str>) -> Option<String> {
    let workspace_url = workspace_url?;
    let channel_id = channel_id?;
    if workspace_url.trim().is_empty() || channel_id.trim().is_empty() {
        return None;
    }
    Some(format!("{}/archives/{}", workspace_url.trim_end_matches('/'), channel_id))
}

/// `{workspaceUrl}/archives/{channelId}/p{ts-without-dot}`.
pub fn build_permalink_uri(
    workspace_url: Option<&str>,
    channel_id: Option<&str>,
    ts: Option<&str>,
) -> Option<String> {
    let workspace_url = workspace_url?;
    let channel_id = channel_id?;
    let ts = ts?;
    if workspace_url.trim().is_empty() || channel_id.trim().is_empty() || ts.trim().is_empty() {
        return None;
    }
    let normalized_ts = ts.replace('.', "");
    Some(format!(
        "{}/archives/{}/p{}",
        workspace_url.trim_end_matches('/'),
        channel_id,
        normalized_ts
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_uri_trims_trailing_slash() {
        assert_eq!(
            build_channel_uri(Some("https://foo.slack.com/"), Some("C123")).as_deref(),
            Some("https://foo.slack.com/archives/C123")
        );
    }

    #[test]
    fn channel_uri_none_on_missing_input() {
        assert_eq!(build_channel_uri(None, Some("C123")), None);
        assert_eq!(build_channel_uri(Some("https://foo.slack.com"), None), None);
    }

    #[test]
    fn permalink_strips_dot_from_ts() {
        assert_eq!(
            build_permalink_uri(Some("https://foo.slack.com"), Some("C123"), Some("1234567890.123456"))
                .as_deref(),
            Some("https://foo.slack.com/archives/C123/p1234567890123456")
        );
    }
}
