//! Custom board persistence. Same conceptual schema as the legacy
//! `custom-board.json` (`CustomBoardStorageService.cs`), but channel
//! membership is keyed by channel ID rather than name — the legacy
//! name-based scheme silently broke when a channel was renamed.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};

const FILE_NAME: &str = "custom-board.json";
const DEFAULT_BOARD_NAME: &str = "カスタムボード";
const MAX_BOARDS: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomBoardDefinition {
    pub name: String,
    pub channel_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomBoardState {
    #[serde(default = "default_filter")]
    pub active_filter: String,
    #[serde(default = "default_active_index")]
    pub active_custom_board_index: i32,
    #[serde(default)]
    pub custom_boards: Vec<CustomBoardDefinition>,
}

fn default_filter() -> String {
    "JoinedOnly".to_string()
}

fn default_active_index() -> i32 {
    -1
}

impl Default for CustomBoardState {
    fn default() -> Self {
        Self {
            active_filter: default_filter(),
            active_custom_board_index: default_active_index(),
            custom_boards: Vec::new(),
        }
    }
}

fn file_path(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Persistence(e.to_string()))?;
    Ok(dir.join(FILE_NAME))
}

pub fn load(app: &AppHandle) -> AppResult<CustomBoardState> {
    let path = file_path(app)?;
    if !path.exists() {
        return Ok(CustomBoardState::default());
    }
    let text = fs::read_to_string(&path).map_err(|e| AppError::Persistence(e.to_string()))?;
    Ok(serde_json::from_str(&text).unwrap_or_default())
}

/// Normalizes names, de-dupes channel ids, drops boards left with zero
/// channels, and caps at `MAX_BOARDS` before writing.
pub fn save(app: &AppHandle, mut state: CustomBoardState) -> AppResult<()> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Persistence(e.to_string()))?;
    fs::create_dir_all(&dir).map_err(|e| AppError::Persistence(e.to_string()))?;

    for board in &mut state.custom_boards {
        let trimmed = board.name.trim();
        board.name = if trimmed.is_empty() {
            DEFAULT_BOARD_NAME.to_string()
        } else {
            trimmed.to_string()
        };
        let mut seen = HashSet::new();
        board
            .channel_ids
            .retain(|id| !id.trim().is_empty() && seen.insert(id.clone()));
    }
    state.custom_boards.retain(|board| !board.channel_ids.is_empty());
    state.custom_boards.truncate(MAX_BOARDS);

    let path = dir.join(FILE_NAME);
    let json = serde_json::to_string_pretty(&state).map_err(|e| AppError::Persistence(e.to_string()))?;
    fs::write(&path, json).map_err(|e| AppError::Persistence(e.to_string()))?;
    Ok(())
}
