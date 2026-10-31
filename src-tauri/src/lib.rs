mod commands;
mod error;
mod logging;
mod persistence;
mod slack;
mod state;

use tauri::Manager;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .setup(|app| {
            let guard = logging::init(app.handle());
            app.manage(guard);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::auth::authenticate,
            commands::auth::try_auto_login,
            commands::auth::logout,
            commands::channels::refresh_workspace_data,
            commands::channels::refresh_channel,
            commands::channels::load_channels,
            commands::channels::list_all_channels,
            commands::channels::get_user_profile,
            commands::channels::get_emoji_map,
            commands::channels::get_logs,
            commands::messages::post_message,
            commands::reactions::toggle_reaction,
            commands::image::fetch_image,
            commands::boards::load_custom_boards,
            commands::boards::save_custom_boards,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
