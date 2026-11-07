//! Custom board persistence. Same conceptual schema as the legacy
//! `custom-board.json` (`CustomBoardStorageService.cs`), but channel
//! membership is keyed by channel ID rather than name — the legacy
//! name-based scheme silently broke when a channel was renamed.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use slacksitter_core::error::{AppError, AppResult};

const FILE_NAME: &str = "custom-board.json";
const DEFAULT_BOARD_NAME: &str = "カスタムボード";
const MAX_BOARDS: usize = 10;
const KNOWN_FILTERS: [&str; 4] = ["All", "JoinedOnly", "NotJoinedOnly", "CustomOnly"];

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

/// A missing or unparsable file yields the default state rather than an error,
/// so a corrupt `custom-board.json` never blocks startup.
pub fn load(app: &AppHandle) -> AppResult<CustomBoardState> {
    let path = file_path(app)?;
    if !path.exists() {
        return Ok(CustomBoardState::default());
    }
    let text = fs::read_to_string(&path).map_err(|e| AppError::Persistence(e.to_string()))?;
    let state: CustomBoardState = serde_json::from_str(&text).unwrap_or_default();
    Ok(normalize(state))
}

/// Normalizes names, de-dupes channel ids, drops boards left with zero
/// channels, caps at `MAX_BOARDS`, and keeps the active index / filter
/// consistent with the surviving boards.
pub fn normalize(mut state: CustomBoardState) -> CustomBoardState {
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

    state.active_custom_board_index = if state.custom_boards.is_empty() {
        -1
    } else {
        state
            .active_custom_board_index
            .clamp(0, state.custom_boards.len() as i32 - 1)
    };

    state.active_filter = match KNOWN_FILTERS
        .iter()
        .find(|known| known.eq_ignore_ascii_case(&state.active_filter))
    {
        // A custom filter with nothing to show would leave an empty board.
        Some(&"CustomOnly") if state.custom_boards.is_empty() => default_filter(),
        Some(known) => (*known).to_string(),
        None => default_filter(),
    };
    state
}

pub fn save(app: &AppHandle, state: CustomBoardState) -> AppResult<()> {
    let state = normalize(state);
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Persistence(e.to_string()))?;
    fs::create_dir_all(&dir).map_err(|e| AppError::Persistence(e.to_string()))?;

    let path = dir.join(FILE_NAME);
    let json = serde_json::to_string_pretty(&state).map_err(|e| AppError::Persistence(e.to_string()))?;
    fs::write(&path, json).map_err(|e| AppError::Persistence(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(name: &str, ids: &[&str]) -> CustomBoardDefinition {
        CustomBoardDefinition {
            name: name.to_string(),
            channel_ids: ids.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn state(filter: &str, index: i32, boards: Vec<CustomBoardDefinition>) -> CustomBoardState {
        CustomBoardState {
            active_filter: filter.to_string(),
            active_custom_board_index: index,
            custom_boards: boards,
        }
    }

    #[test]
    fn blank_name_falls_back_and_name_is_trimmed() {
        let out = normalize(state("All", 0, vec![board("  ", &["C1"]), board("  dev  ", &["C2"])]));
        assert_eq!(out.custom_boards[0].name, DEFAULT_BOARD_NAME);
        assert_eq!(out.custom_boards[1].name, "dev");
    }

    #[test]
    fn duplicate_and_blank_channel_ids_are_dropped_in_order() {
        let out = normalize(state("All", 0, vec![board("a", &["C2", "C1", "C2", " ", "C1"])]));
        assert_eq!(out.custom_boards[0].channel_ids, vec!["C2", "C1"]);
    }

    #[test]
    fn empty_boards_are_pruned() {
        let out = normalize(state("All", 0, vec![board("a", &[]), board("b", &["C1"])]));
        assert_eq!(out.custom_boards.len(), 1);
        assert_eq!(out.custom_boards[0].name, "b");
    }

    #[test]
    fn at_most_ten_boards_are_kept() {
        let boards = (0..12).map(|i| board(&format!("b{i}"), &["C1"])).collect();
        let out = normalize(state("All", 0, boards));
        assert_eq!(out.custom_boards.len(), MAX_BOARDS);
        assert_eq!(out.custom_boards[9].name, "b9");
    }

    #[test]
    fn active_index_is_clamped_to_surviving_boards() {
        let boards = || vec![board("a", &["C1"]), board("b", &["C2"])];
        assert_eq!(normalize(state("All", 99, boards())).active_custom_board_index, 1);
        assert_eq!(normalize(state("All", -1, boards())).active_custom_board_index, 0);
        assert_eq!(normalize(state("All", 5, vec![])).active_custom_board_index, -1);
    }

    #[test]
    fn custom_filter_without_boards_falls_back_to_joined_only() {
        assert_eq!(normalize(state("CustomOnly", -1, vec![])).active_filter, "JoinedOnly");
        assert_eq!(
            normalize(state("CustomOnly", 0, vec![board("a", &["C1"])])).active_filter,
            "CustomOnly"
        );
    }

    #[test]
    fn filter_is_canonicalized_and_unknown_values_fall_back() {
        assert_eq!(normalize(state("notjoinedonly", -1, vec![])).active_filter, "NotJoinedOnly");
        assert_eq!(normalize(state("bogus", -1, vec![])).active_filter, "JoinedOnly");
    }

    #[test]
    fn missing_fields_deserialize_with_defaults() {
        let parsed: CustomBoardState = serde_json::from_str(r#"{"customBoards":[]}"#).unwrap();
        assert_eq!(parsed.active_filter, "JoinedOnly");
        assert_eq!(parsed.active_custom_board_index, -1);
    }
}
