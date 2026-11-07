use tauri::AppHandle;

use crate::error::AppResult;
use crate::persistence::boards::{self, CustomBoardState};

#[tauri::command]
pub fn load_custom_boards(app: AppHandle) -> AppResult<CustomBoardState> {
    boards::load(&app)
}

#[tauri::command]
pub fn save_custom_boards(app: AppHandle, board_state: CustomBoardState) -> AppResult<()> {
    boards::save(&app, board_state)
}
