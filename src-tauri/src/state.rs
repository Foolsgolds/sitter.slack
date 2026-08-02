use std::collections::HashMap;
use std::sync::RwLock;

use tokio::sync::Semaphore;

use crate::slack::client::SlackClient;
use crate::slack::models::ChannelSummary;

/// Max concurrent full-size inline image downloads triggered by `fetch_image`.
const MAX_CONCURRENT_INLINE_IMAGE_LOADS: usize = 2;

/// Process-wide app state, managed by Tauri and shared across command invocations.
pub struct AppState {
    pub client: RwLock<Option<SlackClient>>,
    pub workspace_url: RwLock<Option<String>>,
    pub current_user_id: RwLock<Option<String>>,
    /// userId -> resolved avatar URL (None cached for lookups that failed, to avoid retrying).
    pub user_image_cache: RwLock<HashMap<String, Option<String>>>,
    /// Flattened custom-emoji name -> URL map, rebuilt on every emoji.list refresh.
    pub emoji_map: RwLock<HashMap<String, String>>,
    /// All unarchived channels regardless of name, used for the add-channel
    /// picker and message-post channel-name resolution.
    pub channel_catalog: RwLock<Vec<ChannelSummary>>,
    pub image_semaphore: Semaphore,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            client: RwLock::new(None),
            workspace_url: RwLock::new(None),
            current_user_id: RwLock::new(None),
            user_image_cache: RwLock::new(HashMap::new()),
            emoji_map: RwLock::new(HashMap::new()),
            channel_catalog: RwLock::new(Vec::new()),
            image_semaphore: Semaphore::new(MAX_CONCURRENT_INLINE_IMAGE_LOADS),
        }
    }
}
