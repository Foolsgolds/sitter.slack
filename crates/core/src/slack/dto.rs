use serde::Deserialize;

/// Raw Slack `conversations.*` channel object. Permissive: unknown/renamed
/// fields must not break deserialization, so everything optional-ish.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SlackConversation {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub is_member: bool,
    #[serde(default)]
    pub is_archived: bool,
    #[serde(default)]
    pub num_members: Option<i64>,
    #[serde(default)]
    pub topic: Option<SlackTopicOrPurpose>,
    #[serde(default)]
    pub purpose: Option<SlackTopicOrPurpose>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SlackTopicOrPurpose {
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SlackFile {
    pub id: Option<String>,
    pub mimetype: Option<String>,
    pub thumb_360: Option<String>,
    pub thumb_480: Option<String>,
    pub thumb_160: Option<String>,
    pub thumb_80: Option<String>,
    pub thumb_64: Option<String>,
    pub url_private_download: Option<String>,
    pub url_private: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SlackReaction {
    pub name: Option<String>,
    #[serde(default)]
    pub count: i64,
    #[serde(default)]
    pub users: Vec<String>,
}

/// Raw Slack message event, as returned by conversations.history/replies.
/// `attachments`/`blocks` are kept as raw JSON since their shape varies
/// across Slack API versions and we only ever probe a handful of fields.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SlackMessage {
    pub user: Option<String>,
    pub ts: Option<String>,
    pub text: Option<String>,
    #[serde(default)]
    pub files: Vec<SlackFile>,
    #[serde(default)]
    pub attachments: Vec<serde_json::Value>,
    #[serde(default)]
    pub blocks: Vec<serde_json::Value>,
    #[serde(default)]
    pub reactions: Vec<SlackReaction>,
    #[serde(default)]
    pub reply_count: i64,
    pub thread_ts: Option<String>,
    pub subtype: Option<String>,
    pub latest_reply: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SlackUserProfile {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub real_name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub status_text: String,
    pub image_192: Option<String>,
    pub image_72: Option<String>,
    pub image_48: Option<String>,
    pub image_32: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SlackUser {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub profile: SlackUserProfile,
}
