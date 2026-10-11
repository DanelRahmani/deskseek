mod api;
mod commands;
mod conversation;
mod store;

use std::sync::Mutex;

use tauri::Manager;

pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
    pub http: reqwest::Client,
    pub paths: store::paths::AppPaths,
    pub settings: Mutex<store::settings::Settings>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let root = app.path().app_data_dir()?;
            let paths = store::paths::AppPaths::new(root);
            paths.ensure()?;
            let conn = store::db::open(&paths.db_file())?;
            let settings = store::settings::load(&paths.settings_file());
            let http = api::client::build_http();
            app.manage(AppState {
                db: Mutex::new(conn),
                http,
                paths,
                settings: Mutex::new(settings),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::has_api_key,
            commands::masked_api_key,
            commands::set_api_key,
            commands::clear_api_key,
            commands::test_connection,
            commands::list_models,
            commands::list_chats,
            commands::create_chat,
            commands::get_messages,
            commands::rename_chat,
            commands::delete_chat,
            commands::set_chat_model,
            commands::send_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
