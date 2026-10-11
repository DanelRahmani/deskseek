mod api;
mod approvals;
mod commands;
mod conversation;
mod mcp;
mod memory;
mod runs;
mod skills;
mod store;
mod tools;

use std::sync::{Arc, Mutex};

use tauri::Manager;

pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
    pub http: reqwest::Client,
    pub paths: store::paths::AppPaths,
    pub settings: Mutex<store::settings::Settings>,
    pub approvals: approvals::Approvals,
    /// Live agent runs, so the UI can stop one mid-stream.
    pub runs: runs::Runs,
    /// Long-running, so it waits on an async lock rather than blocking the UI.
    pub mcp: Arc<tokio::sync::RwLock<mcp::McpManager>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let root = app.path().app_data_dir()?;
            let paths = store::paths::AppPaths::new(root);
            paths.ensure()?;
            // Older builds used a lowercase memory.md — keep those facts.
            paths.migrate_legacy();
            let conn = store::db::open(&paths.db_file())?;
            let settings = store::settings::load(&paths.settings_file());
            let http = api::client::build_http();

            // Example skills are written once; a user's edits are never clobbered.
            if let Err(err) = skills::bundle_examples(&paths.skills_dir()) {
                eprintln!("could not write example skills: {err}");
            }

            let mcp_manager = Arc::new(tokio::sync::RwLock::new(mcp::McpManager::new(
                paths.mcp_file(),
            )));

            app.manage(AppState {
                db: Mutex::new(conn),
                http,
                paths,
                settings: Mutex::new(settings),
                approvals: approvals::Approvals::default(),
                runs: runs::Runs::default(),
                mcp: mcp_manager.clone(),
            });

            // Starting MCP servers can involve downloading a package, so it
            // happens in the background: the window opens immediately.
            tauri::async_runtime::spawn(async move {
                mcp_manager.write().await.load().await;
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
            commands::set_chat_permission_mode,
            commands::set_chat_workspace,
            commands::get_memory,
            commands::set_memory,
            commands::memory_path,
            commands::append_memory,
            commands::list_proposals,
            commands::accept_proposals,
            commands::reject_proposals,
            commands::summarize_chat,
            commands::compact_chat,
            commands::documents_dir,
            commands::chatlog_dir,
            commands::get_summary,
            commands::list_skills,
            commands::set_chat_skills,
            commands::respond_approval,
            commands::list_tool_permissions,
            commands::revoke_tool_permission,
            commands::send_message,
            commands::stop_chat,
            commands::running_chats,
            commands::mcp_status,
            commands::mcp_config_path,
            commands::mcp_servers,
            commands::mcp_save_servers,
            commands::mcp_reload,
            commands::mcp_restart,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|handle, event| {
            // MCP servers are child processes; none may outlive the app.
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = handle.try_state::<AppState>() {
                    let mcp = state.mcp.clone();
                    tauri::async_runtime::block_on(async move {
                        mcp.write().await.stop_all().await;
                    });
                }
            }
        });
}
