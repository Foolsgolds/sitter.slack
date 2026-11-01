mod commands;
mod error;
mod logging;
mod persistence;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let guard = logging::init(app.handle());
            app.manage(guard);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::server_settings::load_server_settings,
            commands::server_settings::save_server_settings,
            commands::server_settings::clear_server_settings,
            commands::logs::get_logs,
            commands::logs::write_log,
            commands::boards::load_custom_boards,
            commands::boards::save_custom_boards,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
