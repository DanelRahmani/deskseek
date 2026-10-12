mod api;
mod approvals;
mod backup;
mod commands;
mod conversation;
mod design;
mod fs_watch;
mod mcp;
mod memory;
mod memory_files;
mod notify;
mod prompts;
mod questions;
mod runs;
mod schedule;
mod skills;
mod store;
mod tools;

use std::sync::{Arc, Mutex};

use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
    pub http: reqwest::Client,
    pub paths: store::paths::AppPaths,
    pub settings: Mutex<store::settings::Settings>,
    pub approvals: approvals::Approvals,
    /// Questions the model is waiting on, so the UI can answer one mid-run.
    pub questions: questions::Questions,
    /// Live agent runs, so the UI can stop one mid-stream.
    pub runs: runs::Runs,
    /// Watches the open workspace so the file tree updates by itself.
    pub files: fs_watch::FileWatcher,
    /// What this machine can sandbox with, probed in the background at startup
    /// and again whenever the setup screen re-checks. Read per run rather than
    /// probed per command: `docker info` costs a second when it fails.
    pub sandbox: Arc<tokio::sync::RwLock<tools::sandbox_exec::Capability>>,
    /// The automation browser, started on first use and kept for the session so
    /// a page opened in one turn is still there in the next.
    pub browser: tools::browser::Browser,
    /// Stopping everything that touches the desktop, plus the pacing between
    /// actions. Process-wide on purpose: the chat you cannot see is the one you
    /// most want stopped.
    pub computer: tools::desktop::Gate,
    /// Long-running, so it waits on an async lock rather than blocking the UI.
    pub mcp: Arc<tokio::sync::RwLock<mcp::McpManager>>,
}

/// One tick of the scheduler: fire whatever is due.
///
/// A function rather than a closure inside the spawn, because it takes the app
/// handle by reference: the state guard borrows from it, and that borrow is held
/// across an await, which a closure capturing its own handle could not do.
async fn scheduled_tick(handle: &tauri::AppHandle) {
    let state = handle.state::<AppState>();
    let due = {
        let conn = state.db.lock().unwrap();
        store::db::due_scheduled(&conn, store::db::now_ts()).unwrap_or_default()
    };
    for task in due {
        // Move it on *first*: a run that outlasts this tick must not be started
        // again by the next one.
        let sched = schedule::Schedule::parse(&task.freq, &task.time, task.weekday, task.nth);
        let next = schedule::next_run_at(chrono::Local::now(), &sched);
        {
            let conn = state.db.lock().unwrap();
            if let Err(err) =
                store::db::mark_scheduled_ran(&conn, &task.id, next, store::db::now_ts())
            {
                eprintln!("could not advance scheduled task {}: {err}", task.id);
                continue;
            }
        }
        if let Err(err) = commands::fire_scheduled(handle, &state, &task).await {
            eprintln!("scheduled task {} failed: {err}", task.id);
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    // The press is the event; the release is just the key coming up.
                    if event.state != tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        return;
                    }
                    let Some(state) = app.try_state::<AppState>() else {
                        return;
                    };
                    let stopped = state.runs.cancel_all();
                    let just_paused = state.computer.pause();
                    // A second press with nothing running is not news worth showing.
                    if !just_paused && stopped == 0 {
                        return;
                    }
                    let also = if stopped > 0 {
                        format!(" and stopped {stopped} run(s)")
                    } else {
                        String::new()
                    };
                    let _ = app.emit(
                        tools::desktop::EVENT_COMPUTER,
                        serde_json::json!({
                            "paused": true,
                            "reason": format!("Computer control was stopped with Ctrl+Alt+Esc{also}."),
                        }),
                    );
                })
                .build(),
        )
        .setup(|app| {
            let root = app.path().app_data_dir()?;
            let paths = store::paths::AppPaths::new(root);
            paths.ensure()?;
            // Older builds used a lowercase memory.md — keep those facts.
            paths.migrate_legacy();
            let conn = store::db::open(&paths.db_file())?;
            // A park only means something while the run that made it is in
            // memory. Anything still marked pending is from a process that is
            // gone, so it becomes a notice rather than a button that would do
            // nothing — a scheduled run that was interrupted still gets to say
            // what it wanted.
            if let Ok(lost) = store::db::lose_parked_in_flight(&conn) {
                if lost > 0 {
                    eprintln!("{lost} scheduled run(s) were left waiting by the last session");
                }
            }
            let settings = store::settings::load(&paths.settings_file());
            let http = api::client::build_http();

            // Example skills are written once; a user's edits are never clobbered.
            if let Err(err) = skills::bundle_examples(&paths.skills_dir()) {
                eprintln!("could not write example skills: {err}");
            }

            let mcp_manager = Arc::new(tokio::sync::RwLock::new(mcp::McpManager::new(
                paths.mcp_file(),
                Some(app.handle().clone()),
            )));            let sandbox = Arc::new(tokio::sync::RwLock::new(
                tools::sandbox_exec::Capability::default(),
            ));
            let sandbox_probe = sandbox.clone();

            app.manage(AppState {
                db: Mutex::new(conn),
                http,
                paths,
                settings: Mutex::new(settings),
                approvals: approvals::Approvals::default(),
                questions: questions::Questions::default(),
                runs: runs::Runs::default(),
                files: fs_watch::FileWatcher::new(app.handle().clone()),
                sandbox,
                browser: tools::browser::Browser::default(),
                computer: tools::desktop::Gate::default(),
                mcp: mcp_manager.clone(),
            });

            // Ctrl+Alt+Esc stops everything that touches the desktop, including a
            // run that has taken the mouse. Registering it can fail (another app
            // may already own the combination), and that must not stop the app
            // starting — it is reported and the run is simply not guardable.
            let kill_switch = tauri_plugin_global_shortcut::Shortcut::new(
                Some(
                    tauri_plugin_global_shortcut::Modifiers::CONTROL
                        | tauri_plugin_global_shortcut::Modifiers::ALT,
                ),
                tauri_plugin_global_shortcut::Code::Escape,
            );
            if let Err(err) = app.global_shortcut().register(kill_switch) {
                eprintln!("could not register the Ctrl+Alt+Esc kill switch: {err}");
            }

            // Starting MCP servers can involve downloading a package, so it
            // happens in the background: the window opens immediately.
            tauri::async_runtime::spawn(async move {
                mcp_manager.write().await.load().await;
            });

            // Probing Docker and WSL can take up to fifteen seconds when the
            // engine is installed but asleep, which must never delay the window.
            tauri::async_runtime::spawn(async move {
                *sandbox_probe.write().await = tools::sandbox_exec::detect().await;
            });

            // The scheduled-task ticker. Half a minute, against a schedule
            // grammar whose finest unit is a minute: a task fires within a minute
            // of its time, and the loop costs one indexed query when nothing is
            // due. A missed window (the app was closed) fires once on the next
            // tick rather than catching up — `mark_scheduled_ran` moves the time
            // on before the run starts.
            let ticker = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut beat = tokio::time::interval(std::time::Duration::from_secs(30));
                loop {
                    beat.tick().await;
                    scheduled_tick(&ticker).await;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::backup_profile,
            commands::restore_profile,
            commands::has_api_key,
            commands::masked_api_key,
            commands::set_api_key,
            commands::clear_api_key,
            commands::test_connection,
            commands::list_models,
            commands::list_chats,
            commands::create_chat,
            commands::get_messages,
            commands::search_messages,
            commands::rewind_last_run,
            commands::resend_last_turn,
            commands::restore_discarded_tail,
            commands::rename_chat,
            commands::delete_chat,
            commands::duplicate_chat,
            commands::set_chat_archived,
            commands::set_chat_model,
            commands::set_chat_permission_mode,
            commands::set_chat_mode,
            commands::set_chat_workspace,
            commands::get_memory,
            commands::set_memory,
            commands::memory_path,
            commands::append_memory,
            commands::list_proposals,
            commands::accept_proposals,
            commands::reject_proposals,
            commands::memory_overview,
            commands::delete_memory_entry,
            commands::summarize_chat,
            commands::compact_chat,
            commands::documents_dir,
            commands::export_chat,
            commands::export_chat_json,
            commands::import_chat_json,
            commands::export_chat_pdf,
            commands::chatlog_dir,
            commands::open_document,
            commands::context_breakdown,
            commands::reveal_path,
            commands::start_docker,
            commands::list_artifacts,
            commands::get_artifact,
            commands::save_artifact_version,
            commands::list_projects,
            commands::create_project,
            commands::rename_project,
            commands::set_project_folder,
            commands::delete_project,
            commands::set_chat_project,
            commands::set_project_instructions,
            commands::project_spend,
            commands::project_memory,
            commands::list_parked,
            commands::dismiss_parked,
            commands::list_all_artifacts,
            commands::list_design_systems,
            commands::save_design_system,
            commands::design_system_from_project,
            commands::delete_design_system,
            commands::design_dir,
            commands::list_prompts,
            commands::save_prompt,
            commands::delete_prompt,
            commands::prompts_dir,
            commands::get_summary,
            commands::list_skills,
            commands::set_chat_skills,
            commands::set_chat_designs,
            commands::set_skill_default,
            commands::skills_dir,
            commands::install_skill,
            commands::install_skill_text,
            commands::install_examples,
            commands::delete_skill,
            commands::attach_path,
            commands::attach_bytes,
            commands::chat_folder,
            commands::respond_approval,
            commands::respond_question,
            commands::list_tool_permissions,
            commands::revoke_tool_permission,
            commands::tool_grants,
            commands::spend_report,
            commands::send_message,
            commands::stop_chat,
            commands::running_chats,
            commands::interrupted_runs,
            commands::dismiss_interrupted,
            commands::list_scheduled,
            commands::create_scheduled,
            commands::update_scheduled,
            commands::delete_scheduled,
            commands::run_scheduled_now,
            commands::probe_environment,
            commands::computer_state,
            commands::set_computer_paused,
            commands::list_tree,
            commands::read_preview,
            commands::watch_workspace,
            commands::unwatch_workspace,
            commands::mcp_status,
            commands::mcp_config_path,
            commands::mcp_servers,
            commands::mcp_save_servers,
            commands::mcp_reload,
            commands::mcp_restart,
            commands::mcp_logs,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|handle, event| {
            // MCP servers and the automation browser are child processes; none
            // may outlive the app.
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = handle.try_state::<AppState>() {
                    state.files.clear();
                    let mcp = state.mcp.clone();
                    let browser = &state.browser;
                    tauri::async_runtime::block_on(async move {
                        mcp.write().await.stop_all().await;
                        browser.stop().await;
                    });
                }
            }
        });
}
